//! MRATE Engine
//! 
//! Core scoring logic for the Macro-Regime Adaptive Trading Engine

use chrono::Utc;
use tracing::info;

use crate::broker::OandaClient;
use crate::feeds::{VixClient, FredClient, CryptoClient, EconomicCalendarClient, DxyClient, FearGreedClient, GldClient, RedditSentimentClient, AlternativeMeClient};
use crate::macro_sentiment::PolymarketClient;
use crate::models::Candle;
use crate::news::{AlphaVantageClient, NewsApiClient, RegimeEvent};

use super::models::{
    MrateInputs, MrateOutput, Regime, StrategyWeights, TrendDirection, FavoredInstrument, DataHealth, RegimeState,
    InstrumentScores, InstrumentScore, InstrumentRecommendation, PriceRegime,
    InstrumentTechnicals, TradingDirection, StrategyCategory,
    TimeframeTechnicals, MultiTimeframeData,
    derive_direction_mtf,
};
use super::thresholds::DynamicThresholds;
use std::sync::Mutex;

/// MRATE Engine - calculates scores and determines regime
pub struct MrateEngine {
    polymarket: PolymarketClient,
    vix_client: VixClient,
    fred_client: FredClient,
    crypto_client: CryptoClient,
    calendar_client: EconomicCalendarClient,
    dxy_client: DxyClient,
    fear_greed_client: FearGreedClient,
    gld_client: GldClient,
    /// News sentiment client (Alpha Vantage - legacy)
    news_client: Option<AlphaVantageClient>,
    /// NewsAPI client with regime event detection
    newsapi_client: Option<NewsApiClient>,
    /// Reddit sentiment client
    reddit_client: RedditSentimentClient,
    /// Alternative.me Fear & Greed (backup)
    alt_fear_greed_client: AlternativeMeClient,
    /// Regime state for hysteresis (thread-safe)
    regime_state: Mutex<RegimeState>,
}

impl MrateEngine {
    pub fn new() -> Self {
        // Try to load Alpha Vantage API key from environment (legacy)
        let news_client = std::env::var("ALPHA_VANTAGE_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .map(AlphaVantageClient::new);
        
        // Try to load NewsAPI key (primary news source with regime detection)
        let newsapi_client = std::env::var("NEWSAPI_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .map(NewsApiClient::new);
        
        if newsapi_client.is_some() {
            info!("📰 NewsAPI client initialized - regime event detection enabled");
        } else {
            tracing::warn!("⚠️ NEWSAPI_KEY not set - regime event detection disabled. Get a free key at https://newsapi.org");
        }
        
        Self {
            polymarket: PolymarketClient::new(),
            vix_client: VixClient::new(),
            fred_client: FredClient::new(),
            crypto_client: CryptoClient::new(),
            calendar_client: EconomicCalendarClient::new(),
            dxy_client: DxyClient::new(),
            fear_greed_client: FearGreedClient::new(),
            gld_client: GldClient::new(),
            news_client,
            newsapi_client,
            reddit_client: RedditSentimentClient::new(),
            alt_fear_greed_client: AlternativeMeClient::new(),
            regime_state: Mutex::new(RegimeState::default()),
        }
    }
    
    /// Run full MRATE calculation pipeline
    pub async fn calculate(&self, oanda: Option<&OandaClient>) -> MrateOutput {
        // Step 1: Gather all inputs and track data health
        let (inputs, data_health) = self.gather_inputs(oanda).await;
        let degraded_mode = data_health.is_degraded();
        
        // Step 2: Calculate scores
        let liquidity_score = self.calculate_liquidity_score(&inputs);
        let risk_score = self.calculate_risk_score(&inputs);
        let uncertainty_score = self.calculate_uncertainty_score(&inputs);
        
        // Step 3: Determine regime (with hysteresis)
        let proposed_regime = self.calculate_raw_regime(liquidity_score, risk_score);
        let (regime, regime_change_pending) = self.apply_regime_hysteresis(proposed_regime);
        
        // Step 4: Get strategy weights for regime
        let strategy_weights = StrategyWeights::for_regime(regime);
        
        // Step 5: Calculate risk multiplier (reduce if degraded or regime event detected)
        let base_multiplier = self.calculate_risk_multiplier(uncertainty_score);
        let mut risk_multiplier = if degraded_mode {
            (base_multiplier * 0.7).max(0.3) // 30% reduction in degraded mode
        } else {
            base_multiplier
        };
        
        // Apply regime event risk reduction (Fed chair change, crisis, etc.)
        if inputs.regime_event_risk_reduction < 1.0 {
            let original = risk_multiplier;
            risk_multiplier = (risk_multiplier * inputs.regime_event_risk_reduction).max(0.2);
            info!(
                "🚨 Regime event reducing risk multiplier: {:.2} -> {:.2} (reduction: {:.0}%)",
                original,
                risk_multiplier,
                (1.0 - inputs.regime_event_risk_reduction) * 100.0
            );
        }
        
        // Step 6: Calculate instrument scores (new multi-instrument scoring)
        let instrument_scores = self.calculate_instrument_scores(regime, &inputs, liquidity_score, risk_score);
        
        // Step 6b: Determine favored instrument (uses detailed logic with real yield awareness)
        let favored_instrument = self.determine_favored_instrument(regime, &inputs);
        
        // Step 7: Calculate dynamic thresholds (DTE)
        let thresholds = DynamicThresholds::calculate(liquidity_score, risk_score, uncertainty_score);
        
        // Step 8: Calculate regime confidence (distance from boundary thresholds)
        let regime_confidence = self.calculate_regime_confidence(liquidity_score, risk_score, regime);
        
        // Step 9: Record scores and calculate momentum
        let (liquidity_momentum, risk_momentum) = {
            let mut state = self.regime_state.lock().unwrap();
            state.record_scores(liquidity_score, risk_score);
            (state.liquidity_momentum(), state.risk_momentum())
        };
        
        // Log top instruments
        let top = instrument_scores.top_instruments(3);
        info!(
            "MRATE: regime={:?}, liq={:.0}, risk={:.0}, unc={:.0}, mult={:.2}, top_instruments=[{}: {:.0}, {}: {:.0}, {}: {:.0}], degraded={}",
            regime, liquidity_score, risk_score, uncertainty_score, risk_multiplier,
            top.get(0).map(|i| i.symbol.as_str()).unwrap_or("-"), top.get(0).map(|i| i.score).unwrap_or(0.0),
            top.get(1).map(|i| i.symbol.as_str()).unwrap_or("-"), top.get(1).map(|i| i.score).unwrap_or(0.0),
            top.get(2).map(|i| i.symbol.as_str()).unwrap_or("-"), top.get(2).map(|i| i.score).unwrap_or(0.0),
            degraded_mode
        );
        
        MrateOutput {
            timestamp: Utc::now(),
            regime,
            liquidity_score,
            risk_score,
            uncertainty_score,
            risk_multiplier,
            strategy_weights,
            favored_instrument,
            instrument_scores,
            thresholds,
            regime_confidence,
            proposed_regime,
            regime_change_pending,
            liquidity_momentum,
            risk_momentum,
            data_health,
            degraded_mode,
            inputs: Some(inputs),
        }
    }
    
    /// Gather all input data from various sources
    /// Returns (inputs, data_health) tuple
    async fn gather_inputs(&self, oanda: Option<&OandaClient>) -> (MrateInputs, DataHealth) {
        let mut inputs = MrateInputs::default();
        let mut health = DataHealth::default();
        let now = Utc::now();
        
        // Fetch Polymarket data
        let sentiment = self.polymarket.calculate_macro_sentiment().await;
        
        // Check if Polymarket returned meaningful data
        if sentiment.markets_analyzed > 0 {
            health.polymarket_ok = true;
            health.last_polymarket_success = Some(now);
            
            // Fed rate cut probability (0-1 in sentiment, convert to 0-100)
            inputs.rate_cut_probability = Some(sentiment.fed_rate_cut_probability * 100.0);
            
            // BTC sentiment
            inputs.btc_sentiment = Some(sentiment.btc_bullish_probability * 100.0);
            
            // Inflation expectations
            inputs.inflation_expectations = Some(sentiment.inflation_expectation * 100.0);
            
            // Safe haven demand
            inputs.safe_haven_demand = Some(sentiment.safe_haven_demand * 100.0);
            
            // Fed uncertainty
            inputs.fed_uncertainty = Some(sentiment.fed_uncertainty * 100.0);
        } else {
            tracing::warn!("Polymarket returned no data - using fallback values");
        }
        
        // Fetch market trends from OANDA
        let mut oanda_success = false;
        if let Some(client) = oanda {
            // S&P 500
            if let Ok(candles) = client.get_candles("SPX500_USD", "H4", 20).await {
                inputs.sp500_trend = Some(self.calculate_trend(&candles));
                oanda_success = true;
            }
            
            // ═══════════════════════════════════════════════════════════════
            // GOLD (XAU_USD) - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias - what you see on the chart)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("XAU_USD", "D", 250).await {
                inputs.gold_adx_d1 = self.calculate_adx(&candles);
                inputs.gold_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy/ATR percentile)
            if let Ok(candles) = client.get_candles("XAU_USD", "H4", 100).await {
                inputs.atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.gold_adx = self.calculate_adx(&candles);
                inputs.gold_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("XAU_USD", "H1", 100).await {
                inputs.gold_adx_h1 = self.calculate_adx(&candles);
                inputs.gold_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("XAU_USD", "M15", 100).await {
                inputs.gold_adx_m15 = self.calculate_adx(&candles);
                inputs.gold_technicals_m15 = self.calculate_technicals(&candles);
            }
            
            // ═══════════════════════════════════════════════════════════════
            // BITCOIN (BTC_USD) - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("BTC_USD", "D", 250).await {
                inputs.btc_adx_d1 = self.calculate_adx(&candles);
                inputs.btc_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy/trend calculation)
            if let Ok(candles) = client.get_candles("BTC_USD", "H4", 100).await {
                inputs.btc_trend = Some(self.calculate_trend(&candles));
                inputs.btc_atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.btc_adx = self.calculate_adx(&candles);
                inputs.btc_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("BTC_USD", "H1", 100).await {
                inputs.btc_adx_h1 = self.calculate_adx(&candles);
                inputs.btc_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("BTC_USD", "M15", 100).await {
                inputs.btc_adx_m15 = self.calculate_adx(&candles);
                inputs.btc_technicals_m15 = self.calculate_technicals(&candles);
            }
            
            // ═══════════════════════════════════════════════════════════════
            // EUR/USD - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("EUR_USD", "D", 250).await {
                inputs.eur_usd_adx_d1 = self.calculate_adx(&candles);
                inputs.eur_usd_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy)
            if let Ok(candles) = client.get_candles("EUR_USD", "H4", 100).await {
                inputs.eur_usd_trend = Some(self.calculate_trend(&candles));
                inputs.eur_usd_atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.eur_usd_adx = self.calculate_adx(&candles);
                inputs.eur_usd_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("EUR_USD", "H1", 100).await {
                inputs.eur_usd_adx_h1 = self.calculate_adx(&candles);
                inputs.eur_usd_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("EUR_USD", "M15", 100).await {
                inputs.eur_usd_adx_m15 = self.calculate_adx(&candles);
                inputs.eur_usd_technicals_m15 = self.calculate_technicals(&candles);
            }
            
            // ═══════════════════════════════════════════════════════════════
            // USD/JPY - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("USD_JPY", "D", 250).await {
                inputs.usd_jpy_adx_d1 = self.calculate_adx(&candles);
                inputs.usd_jpy_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy)
            if let Ok(candles) = client.get_candles("USD_JPY", "H4", 100).await {
                inputs.usd_jpy_trend = Some(self.calculate_trend(&candles));
                inputs.usd_jpy_atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.usd_jpy_adx = self.calculate_adx(&candles);
                inputs.usd_jpy_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("USD_JPY", "H1", 100).await {
                inputs.usd_jpy_adx_h1 = self.calculate_adx(&candles);
                inputs.usd_jpy_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("USD_JPY", "M15", 100).await {
                inputs.usd_jpy_adx_m15 = self.calculate_adx(&candles);
                inputs.usd_jpy_technicals_m15 = self.calculate_technicals(&candles);
            }
            
            // ═══════════════════════════════════════════════════════════════
            // WTI OIL (WTICO_USD) - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("WTICO_USD", "D", 250).await {
                inputs.oil_adx_d1 = self.calculate_adx(&candles);
                inputs.oil_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy)
            if let Ok(candles) = client.get_candles("WTICO_USD", "H4", 100).await {
                inputs.oil_trend = Some(self.calculate_trend(&candles));
                inputs.oil_atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.oil_adx = self.calculate_adx(&candles);
                inputs.oil_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("WTICO_USD", "H1", 100).await {
                inputs.oil_adx_h1 = self.calculate_adx(&candles);
                inputs.oil_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("WTICO_USD", "M15", 100).await {
                inputs.oil_adx_m15 = self.calculate_adx(&candles);
                inputs.oil_technicals_m15 = self.calculate_technicals(&candles);
            }
            
            // ═══════════════════════════════════════════════════════════════
            // NATURAL GAS (NATGAS_USD) - Multi-Timeframe: D1, H1, M15
            // ═══════════════════════════════════════════════════════════════
            // D1 (Daily - PRIMARY trend bias)
            // Need 250+ candles to calculate true EMA 200
            if let Ok(candles) = client.get_candles("NATGAS_USD", "D", 250).await {
                inputs.natgas_adx_d1 = self.calculate_adx(&candles);
                inputs.natgas_technicals_d1 = self.calculate_technicals(&candles);
                oanda_success = true;
            }
            // H4 (kept for legacy)
            if let Ok(candles) = client.get_candles("NATGAS_USD", "H4", 100).await {
                inputs.natgas_trend = Some(self.calculate_trend(&candles));
                inputs.natgas_atr_percentile = Some(self.calculate_atr_percentile(&candles));
                inputs.natgas_adx = self.calculate_adx(&candles);
                inputs.natgas_technicals = self.calculate_technicals(&candles);
            }
            // H1 (confirmation)
            if let Ok(candles) = client.get_candles("NATGAS_USD", "H1", 100).await {
                inputs.natgas_adx_h1 = self.calculate_adx(&candles);
                inputs.natgas_technicals_h1 = self.calculate_technicals(&candles);
            }
            // M15 (execution timing)
            if let Ok(candles) = client.get_candles("NATGAS_USD", "M15", 100).await {
                inputs.natgas_adx_m15 = self.calculate_adx(&candles);
                inputs.natgas_technicals_m15 = self.calculate_technicals(&candles);
            }
        }
        
        if oanda_success {
            health.oanda_ok = true;
            health.last_oanda_success = Some(now);
        } else if oanda.is_some() {
            tracing::warn!("OANDA returned no data - market trends unavailable");
        }
        
        // Fetch VIX
        if let Some(vix) = self.vix_client.get_vix().await {
            inputs.vix_level = Some(vix);
            health.vix_ok = true;
            health.last_vix_success = Some(now);
        } else {
            tracing::warn!("VIX fetch failed - using fallback value");
        }
        
        // Fetch Treasury yields from FRED
        let treasury = self.fred_client.get_treasury_data().await;
        inputs.yield_10y = treasury.yield_10y;
        inputs.yield_2y = treasury.yield_2y;
        inputs.real_yield_10y = treasury.real_yield_10y;
        
        if treasury.yield_10y.is_some() || treasury.real_yield_10y.is_some() {
            health.fred_ok = true;
            health.last_fred_success = Some(now);
        } else {
            tracing::warn!("FRED returned no data - Treasury yields unavailable");
        }
        
        // Calculate 10Y yield trend from current yield
        // (Simple: below 4% = bullish, above 5% = bearish)
        if let Some(y10) = inputs.yield_10y {
            inputs.yield_10y_trend = Some(if y10 < 3.5 {
                TrendDirection::StrongDown
            } else if y10 < 4.0 {
                TrendDirection::Down
            } else if y10 < 4.5 {
                TrendDirection::Flat
            } else if y10 < 5.0 {
                TrendDirection::Up
            } else {
                TrendDirection::StrongUp
            });
        }
        
        // Fetch DXY (US Dollar Index)
        if let Some(dxy) = self.dxy_client.get_dxy().await {
            inputs.dxy_level = Some(dxy.level);
            // Update DXY trend based on SMA
            inputs.dxy_trend = Some(match DxyClient::dxy_trend(dxy.level, dxy.sma_5d) {
                "STRONG_UP" => TrendDirection::StrongUp,
                "UP" => TrendDirection::Up,
                "STRONG_DOWN" => TrendDirection::StrongDown,
                "DOWN" => TrendDirection::Down,
                _ => TrendDirection::Flat,
            });
            health.dxy_ok = true;
            health.last_dxy_success = Some(now);
        } else {
            tracing::warn!("⚠️ DXY fetch FAILED - this is critical for MRATE accuracy!");
        }
        
        // Fetch Fear & Greed Index
        if let Some(fg) = self.fear_greed_client.get_fear_greed().await {
            inputs.fear_greed_index = Some(fg.value);
            inputs.fear_greed_category = Some(FearGreedClient::get_category(fg.value).to_string());
            health.fear_greed_ok = true;
            health.last_fear_greed_success = Some(now);
        } else {
            tracing::warn!("⚠️ Fear & Greed Index fetch FAILED - sentiment data unavailable!");
        }
        
        // Fetch Alternative.me Fear & Greed (backup/validation)
        if let Some(alt_fg) = self.alt_fear_greed_client.get_fear_greed().await {
            inputs.alt_fear_greed_index = Some(alt_fg.value);
            
            // If primary Fear & Greed failed, use this as fallback
            if inputs.fear_greed_index.is_none() {
                inputs.fear_greed_index = Some(alt_fg.value);
                inputs.fear_greed_category = Some(alt_fg.value_classification);
                health.fear_greed_ok = true;
                health.last_fear_greed_success = Some(now);
                info!("Using Alternative.me Fear & Greed as fallback: {:.0}", alt_fg.value);
            }
        }
        
        // Fetch crypto market data
        let crypto = self.crypto_client.get_market_data().await;
        inputs.stablecoin_dominance = crypto.stablecoin_dominance;
        inputs.btc_exchange_netflow = crypto.btc_exchange_netflow;
        
        // Fetch GLD ETF volume (gold institutional flow proxy)
        if let Some(gld) = self.gld_client.get_gld_flow().await {
            inputs.gold_etf_flow_weekly = Some(gld.volume_ratio);
        }
        
        // Fetch news sentiment and regime events (NewsAPI - primary)
        if let Some(newsapi) = &self.newsapi_client {
            if let Some(analysis) = newsapi.get_analysis().await {
                // Use NewsAPI for sentiment
                inputs.news_sentiment_overall = Some(analysis.overall_sentiment);
                inputs.news_sentiment_gold = Some(analysis.gold_sentiment);
                inputs.news_sentiment_crypto = Some(analysis.crypto_sentiment);
                inputs.news_sentiment_economy = Some(analysis.fed_sentiment);
                inputs.news_bearish_consensus = analysis.bearish_ratio > 0.6;
                inputs.news_bullish_consensus = analysis.bullish_ratio > 0.6;
                
                // REGIME EVENT DETECTION - the key new feature!
                if analysis.regime_event != RegimeEvent::None {
                    inputs.regime_event = Some(format!("{:?}", analysis.regime_event));
                    inputs.regime_event_headline = analysis.regime_event_headline;
                    inputs.regime_event_uncertainty_boost = analysis.regime_event.uncertainty_boost();
                    inputs.regime_event_risk_reduction = analysis.regime_event.risk_reduction();
                    
                    info!(
                        "🚨 REGIME EVENT DETECTED: {:?} - uncertainty boost: {:.0}, risk reduction: {:.2}",
                        analysis.regime_event,
                        inputs.regime_event_uncertainty_boost,
                        inputs.regime_event_risk_reduction
                    );
                }
                
                health.news_ok = true;
                health.last_news_success = Some(now);
            }
        } else if let Some(news) = &self.news_client {
            // Fallback to Alpha Vantage (legacy)
            if let Some(sentiment) = news.get_sentiment().await {
                inputs.news_sentiment_overall = Some(sentiment.overall_score);
                inputs.news_sentiment_gold = sentiment.gold_sentiment;
                inputs.news_sentiment_crypto = sentiment.crypto_sentiment;
                inputs.news_sentiment_economy = sentiment.economy_sentiment;
                inputs.news_sentiment_dispersion = Some(sentiment.sentiment_dispersion);
                inputs.news_bearish_consensus = sentiment.bearish_consensus;
                inputs.news_bullish_consensus = sentiment.bullish_consensus;
                health.news_ok = true;
                health.last_news_success = Some(now);
            } else {
                tracing::warn!("News sentiment fetch failed - proceeding without news data");
            }
        }
        
        // Fetch Reddit sentiment
        if let Some(reddit) = self.reddit_client.get_sentiment().await {
            inputs.reddit_crypto_sentiment = Some(reddit.crypto_sentiment);
            inputs.reddit_btc_sentiment = Some(reddit.btc_sentiment);
            inputs.reddit_gold_sentiment = reddit.gold_sentiment;
            inputs.reddit_dispersion = Some(reddit.dispersion);
            inputs.reddit_bullish_consensus = reddit.bullish_consensus;
            inputs.reddit_bearish_consensus = reddit.bearish_consensus;
            health.reddit_ok = true;
            health.last_reddit_success = Some(now);
            info!(
                "Reddit sentiment: crypto={:.3}, btc={:.3}, gold={:?}, posts={}",
                reddit.crypto_sentiment, reddit.btc_sentiment, reddit.gold_sentiment, reddit.posts_analyzed
            );
        } else {
            tracing::warn!("Reddit sentiment fetch failed - proceeding without Reddit data");
        }
        
        // Fetch economic calendar events
        let calendar_events = self.calendar_client.get_upcoming_events(48).await;
        let hours_to_event = self.calendar_client.hours_to_next_high_impact().await;
        
        if !calendar_events.is_empty() {
            health.calendar_ok = true;
            inputs.high_impact_event_within_24h = calendar_events.iter()
                .any(|e| e.hours_until() <= 24.0);
            inputs.hours_to_next_high_impact = hours_to_event;
            
            if let Some(next_event) = calendar_events.first() {
                inputs.next_event_weight = next_event.event_type.uncertainty_weight();
                info!(
                    "Next high-impact event: {} ({:?}) in {:.1} hours (weight {:.1}x)",
                    next_event.name,
                    next_event.event_type,
                    next_event.hours_until(),
                    next_event.event_type.uncertainty_weight()
                );
            }
        } else {
            // Static fallback was used or no events - still mark as OK
            health.calendar_ok = true;
            inputs.high_impact_event_within_24h = false;
            inputs.hours_to_next_high_impact = None;
        }
        
        (inputs, health)
    }
    
    /// Calculate liquidity score (0-100)
    /// Higher = more liquidity expansion expected
    fn calculate_liquidity_score(&self, inputs: &MrateInputs) -> f64 {
        let mut score = 0.0;
        let mut weight_sum = 0.0;
        
        // Rate cut probability (0.25 weight)
        if let Some(rate_cut) = inputs.rate_cut_probability {
            score += rate_cut * 0.25;
            weight_sum += 0.25;
        }
        
        // Inflation expectations (0.15 weight)
        // Higher inflation expectations = Fed stays hawkish = less liquidity
        if let Some(inflation) = inputs.inflation_expectations {
            score += (100.0 - inflation) * 0.15;
            weight_sum += 0.15;
        }
        
        // Real yields - THE key driver for gold (0.25 weight, increased from 0.20)
        // Negative real yields = bullish gold/liquidity
        if let Some(real_yield) = inputs.real_yield_10y {
            // Real yield -1% = 75, 0% = 50, +2% = 0
            let real_yield_score = (50.0 - real_yield * 25.0).clamp(0.0, 100.0);
            score += real_yield_score * 0.25;
            weight_sum += 0.25;
        } else if let Some(trend) = inputs.yield_10y_trend {
            // Fallback to trend if no real yield
            score += trend.to_inverse_score() * 0.25;
            weight_sum += 0.25;
        } else {
            score += 50.0 * 0.25;
            weight_sum += 0.25;
        }
        
        // DXY trend (0.20 weight, increased from 0.15)
        // USD weakness = more global liquidity
        if let Some(trend) = inputs.dxy_trend {
            score += trend.to_inverse_score() * 0.20;
            weight_sum += 0.20;
        } else {
            score += 50.0 * 0.20;
            weight_sum += 0.20;
        }
        
        // Stablecoin dominance (0.15 weight) - crypto dry powder
        // High stablecoin dom = lots of cash waiting = bullish potential
        if let Some(stable_dom) = inputs.stablecoin_dominance {
            // 4% = 0, 6% = 50, 8% = 100
            let stable_score = ((stable_dom - 4.0) / 4.0 * 100.0).clamp(0.0, 100.0);
            score += stable_score * 0.15;
            weight_sum += 0.15;
        } else {
            score += 50.0 * 0.15;
            weight_sum += 0.15;
        }
        
        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            50.0
        }
    }
    
    /// Calculate risk sentiment score (0-100)
    /// Higher = more fear/risk-off
    fn calculate_risk_score(&self, inputs: &MrateInputs) -> f64 {
        let mut score = 0.0;
        let mut weight_sum = 0.0;
        
        // VIX - primary fear gauge (0.35 weight, increased from 0.30)
        if let Some(vix) = inputs.vix_level {
            // VIX 10 = 0 (no fear), VIX 25 = 50 (moderate), VIX 40+ = 100 (extreme)
            let vix_score = ((vix - 10.0) / 30.0 * 100.0).clamp(0.0, 100.0);
            score += vix_score * 0.35;
            weight_sum += 0.35;
        } else {
            score += 50.0 * 0.35;
            weight_sum += 0.35;
        }
        
        // Safe haven demand (0.30 weight, increased from 0.25)
        if let Some(safe_haven) = inputs.safe_haven_demand {
            score += safe_haven * 0.30;
            weight_sum += 0.30;
        }
        
        // BTC sentiment (0.15 weight)
        // Higher BTC bullishness = lower risk score (more risk-on)
        if let Some(btc) = inputs.btc_sentiment {
            score += (100.0 - btc) * 0.15;
            weight_sum += 0.15;
        }
        
        // BTC exchange netflow (0.05 weight, reduced from 0.10)
        // Positive (inflows to exchanges) = bearish = higher risk
        if let Some(netflow) = inputs.btc_exchange_netflow {
            // -5000 = 0 (bullish), 0 = 50, +5000 = 100 (bearish)
            let flow_score = ((netflow / 5000.0 + 1.0) * 50.0).clamp(0.0, 100.0);
            score += flow_score * 0.05;
            weight_sum += 0.05;
        }
        
        // S&P 500 trend (0.15 weight)
        // S&P falling = higher risk score
        if let Some(trend) = inputs.sp500_trend {
            score += trend.to_inverse_score() * 0.15;
            weight_sum += 0.15;
        } else {
            score += 50.0 * 0.15;
            weight_sum += 0.15;
        }
        
        // Yield curve slope (0.10 weight) — inverted curve is a strong recession signal
        if let (Some(y10), Some(y2)) = (inputs.yield_10y, inputs.yield_2y) {
            let slope = y10 - y2;
            // Slope -0.5% (inverted) = 90, 0% = 50, +1.0% (steep) = 20
            let curve_score = (50.0 - slope * 40.0).clamp(0.0, 100.0);
            score += curve_score * 0.10;
            weight_sum += 0.10;
        }
        
        // Fear & Greed Index (0.08 weight) — use both CNN and Alt.me
        if let Some(fg) = inputs.fear_greed_index {
            let fg_risk = 100.0 - fg;
            score += fg_risk * 0.05;
            weight_sum += 0.05;
        }
        
        // Alternative Fear & Greed (0.03 weight) — crypto-specific confirmation
        if let Some(alt_fg) = inputs.alt_fear_greed_index {
            let alt_fg_risk = 100.0 - alt_fg;
            score += alt_fg_risk * 0.03;
            weight_sum += 0.03;
        }
        
        // DXY level (0.10 weight)
        // High DXY = risk-off (dollar strength), Low DXY = risk-on
        if let Some(dxy) = inputs.dxy_level {
            // DXY 95 = 20 (weak dollar = risk on), DXY 105 = 80 (strong dollar = risk off)
            let dxy_risk = ((dxy - 95.0) / 10.0 * 60.0 + 20.0).clamp(0.0, 100.0);
            score += dxy_risk * 0.10;
            weight_sum += 0.10;
        }
        
        // News sentiment (0.10 weight) — bearish news = higher risk score
        // AV sentiment ranges -1.0 (bearish) to +1.0 (bullish)
        // Convert: -1.0 → 100 risk, 0 → 50, +1.0 → 0 risk
        if let Some(news) = inputs.news_sentiment_overall {
            let news_risk = ((0.5 - news * 0.5) * 100.0).clamp(0.0, 100.0);
            // Boost weight if there's consensus (more reliable signal)
            let news_weight = if inputs.news_bearish_consensus || inputs.news_bullish_consensus {
                0.12
            } else {
                0.10
            };
            score += news_risk * news_weight;
            weight_sum += news_weight;
        }
        
        // Reddit sentiment (0.08 weight) — social media amplifies or contradicts news
        // Reddit -1.0 (bearish) → 100 risk, 0 → 50, +1.0 (bullish) → 0 risk
        if let Some(reddit) = inputs.reddit_crypto_sentiment {
            let reddit_risk = ((0.5 - reddit * 0.5) * 100.0).clamp(0.0, 100.0);
            // Boost weight if there's consensus (more reliable signal)
            let reddit_weight = if inputs.reddit_bearish_consensus || inputs.reddit_bullish_consensus {
                0.10
            } else {
                0.08
            };
            score += reddit_risk * reddit_weight;
            weight_sum += reddit_weight;
        }
        
        // Reddit BTC sentiment (0.05 weight) — BTC-specific social sentiment
        if let Some(reddit_btc) = inputs.reddit_btc_sentiment {
            let btc_risk = ((0.5 - reddit_btc * 0.5) * 100.0).clamp(0.0, 100.0);
            score += btc_risk * 0.05;
            weight_sum += 0.05;
        }
        
        // Reddit Gold sentiment (0.04 weight) — gold-specific social sentiment
        if let Some(reddit_gold) = inputs.reddit_gold_sentiment {
            let gold_risk = ((0.5 - reddit_gold * 0.5) * 100.0).clamp(0.0, 100.0);
            score += gold_risk * 0.04;
            weight_sum += 0.04;
        }
        
        // BTC Trend (0.08 weight) — crypto risk barometer
        // BTC falling = risk-off sentiment
        if let Some(trend) = inputs.btc_trend {
            score += trend.to_inverse_score() * 0.08;
            weight_sum += 0.08;
        }
        
        // BTC ATR Percentile (0.05 weight) — crypto volatility spike = risk
        if let Some(btc_atr) = inputs.btc_atr_percentile {
            // High ATR percentile = high volatility = higher risk
            score += btc_atr * 0.05;
            weight_sum += 0.05;
        }
        
        // Gold ATR Percentile (0.05 weight) — gold volatility spike = uncertainty/risk
        if let Some(gold_atr) = inputs.atr_percentile {
            score += gold_atr * 0.05;
            weight_sum += 0.05;
        }
        
        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            50.0
        }
    }
    
    /// Calculate uncertainty score (0-100)
    /// Higher = more uncertain, trade smaller
    fn calculate_uncertainty_score(&self, inputs: &MrateInputs) -> f64 {
        let mut score = 0.0;
        
        // Fed uncertainty (0.40 weight - increased since it's the primary driver)
        let fed_uncertainty = inputs.fed_uncertainty.unwrap_or(50.0);
        score += fed_uncertainty * 0.40;
        
        // Volatility: Use max(VIX, ATR*0.7) to avoid double-counting correlated signals
        // VIX and ATR are highly correlated during real volatility events
        let vix_unc = if let Some(vix) = inputs.vix_level {
            // VIX > 25 starts adding uncertainty
            ((vix - 15.0) / 25.0 * 100.0).clamp(0.0, 100.0)
        } else {
            50.0
        };
        
        let atr_unc = inputs.atr_percentile.unwrap_or(50.0);
        
        // Take the max of VIX and discounted ATR to avoid double-counting
        let volatility_score = vix_unc.max(atr_unc * 0.7);
        score += volatility_score * 0.25;
        
        // High impact event - graduated scale based on hours until event
        // Event type weighting: FOMC 1.5x, CPI 1.3x, NFP 1.2x etc.
        let base_event_score = match inputs.hours_to_next_high_impact {
            Some(hours) if hours <= 4.0 => 95.0,   // Imminent: very high uncertainty
            Some(hours) if hours <= 12.0 => 75.0,  // Same day: high uncertainty
            Some(hours) if hours <= 24.0 => 55.0,  // Within 24h: moderate uncertainty
            Some(hours) if hours <= 48.0 => 35.0,  // 1-2 days: slight uncertainty
            _ => 15.0,                              // No event: baseline
        };
        let event_score = (base_event_score * inputs.next_event_weight).min(100.0);
        score += event_score * 0.25;
        
        // News sentiment dispersion (0.08 weight) — conflicting news = more uncertainty
        if let Some(dispersion) = inputs.news_sentiment_dispersion {
            // Dispersion 0.0 = 10 (consensus), 0.3 = 50 (mixed), 0.6+ = 90 (conflicting)
            let dispersion_score = (dispersion / 0.6 * 80.0 + 10.0).clamp(10.0, 90.0);
            score += dispersion_score * 0.08;
        } else {
            score += 40.0 * 0.08; // Neutral if no news data
        }
        
        // Dynamic model uncertainty (0.10 weight)
        // Scales with how many critical inputs are missing
        let missing_count = [
            inputs.rate_cut_probability.is_none(),
            inputs.vix_level.is_none(),
            inputs.real_yield_10y.is_none(),
            inputs.dxy_level.is_none(),
            inputs.news_sentiment_overall.is_none() && inputs.reddit_crypto_sentiment.is_none(), // Accept either
        ].iter().filter(|&&m| m).count();
        let model_uncertainty = 20.0 + (missing_count as f64 * 12.0); // 20 if all present, up to 80 if all missing
        score += model_uncertainty * 0.10;
        
        // Reddit sentiment dispersion (0.05 weight) — conflicting social signals = uncertainty
        if let Some(reddit_disp) = inputs.reddit_dispersion {
            // Dispersion 0.0 = 10 (consensus), 0.5 = 50 (mixed), 1.0+ = 90 (chaos)
            let dispersion_score = (reddit_disp / 1.0 * 80.0 + 10.0).clamp(10.0, 90.0);
            score += dispersion_score * 0.05;
        } else {
            score += 40.0 * 0.05; // Neutral if no Reddit data
        }
        
        // REGIME EVENT BOOST - major news events that shift market paradigm
        // Examples: Fed Chair change, bank crisis, major tariff announcements
        // This adds DIRECTLY to uncertainty score (not weighted)
        if inputs.regime_event_uncertainty_boost > 0.0 {
            score += inputs.regime_event_uncertainty_boost;
            info!(
                "🚨 Regime event adding {:.0} to uncertainty score (total now: {:.0})",
                inputs.regime_event_uncertainty_boost,
                score
            );
        }
        
        score.clamp(0.0, 100.0)
    }
    
    /// Calculate raw regime from scores (before hysteresis)
    /// Uses buffer zones to reduce flapping at boundaries
    fn calculate_raw_regime(&self, liquidity: f64, risk: f64) -> Regime {
        // Buffer zones: entry thresholds are 3 points higher than exit thresholds
        // This prevents oscillation at boundaries
        
        // Strong regimes have clear thresholds
        if liquidity > 73.0 && risk > 73.0 {
            return Regime::GoldSuperBull;
        }
        if liquidity > 73.0 && risk < 37.0 {
            return Regime::BtcSuperBull;
        }
        if liquidity < 37.0 && risk > 73.0 {
            return Regime::Panic;
        }
        
        // Trend vs Choppy boundary with buffer
        // Entry to Trend: liquidity > 63
        // Exit from Trend: liquidity < 57
        // This creates a "sticky" zone between 57-63
        let current_regime = self.regime_state.lock().unwrap().current_regime;
        
        if current_regime == Regime::Trend {
            // Already in Trend - need liquidity < 57 to exit
            if liquidity < 57.0 {
                Regime::Choppy
            } else {
                Regime::Trend
            }
        } else {
            // Not in Trend - need liquidity > 63 to enter
            if liquidity > 63.0 {
                Regime::Trend
            } else {
                Regime::Choppy
            }
        }
    }
    
    /// Apply hysteresis to regime changes
    /// Returns (confirmed_regime, change_pending)
    fn apply_regime_hysteresis(&self, proposed: Regime) -> (Regime, bool) {
        let mut state = self.regime_state.lock().unwrap();
        let confirmed = state.update(proposed);
        let pending = state.change_pending();
        (confirmed, pending)
    }
    
    /// Calculate regime confidence (0.0 - 1.0)
    /// Measures how far current scores are from the nearest regime boundary.
    /// High confidence = deep inside a regime; low = near a transition.
    fn calculate_regime_confidence(&self, liquidity: f64, risk: f64, regime: Regime) -> f64 {
        // Distance to the nearest boundary that would change the regime
        let distance = match regime {
            Regime::GoldSuperBull => {
                // Both liq>73 and risk>73 — distance is min margin above 73
                (liquidity - 73.0).min(risk - 73.0).max(0.0)
            }
            Regime::BtcSuperBull => {
                // liq>73, risk<37 — min of (liq-73) and (37-risk)
                (liquidity - 73.0).min(37.0 - risk).max(0.0)
            }
            Regime::Panic => {
                // liq<37, risk>73
                (37.0 - liquidity).min(risk - 73.0).max(0.0)
            }
            Regime::Trend => {
                // liquidity > 57 (sticky exit), not in super-bull/panic zones
                // Distance from the exit boundary at 57
                (liquidity - 57.0).max(0.0)
            }
            Regime::Choppy => {
                // liquidity < 63 (sticky entry to Trend)
                // Distance from the entry boundary at 63
                (63.0 - liquidity).max(0.0)
            }
        };
        // Normalize: 0 at boundary, 1.0 at 27+ points away
        (distance / 27.0).clamp(0.0, 1.0)
    }
    
    /// Calculate risk multiplier from uncertainty
    fn calculate_risk_multiplier(&self, uncertainty: f64) -> f64 {
        // risk_multiplier = 1.5 - (uncertainty / 100)
        // Ranges from 0.5 (high uncertainty) to 1.5 (low uncertainty)
        (1.5 - (uncertainty / 100.0)).clamp(0.3, 1.5)
    }
    
    /// Calculate scores for all tradeable instruments
    /// Each instrument gets 0-100 score based on macro conditions
    fn calculate_instrument_scores(
        &self,
        regime: Regime,
        inputs: &MrateInputs,
        liquidity_score: f64,
        risk_score: f64,
    ) -> InstrumentScores {
        InstrumentScores {
            gold: self.score_gold(regime, inputs, liquidity_score, risk_score),
            bitcoin: self.score_bitcoin(regime, inputs, liquidity_score, risk_score),
            eur_usd: self.score_eur_usd(regime, inputs),
            usd_jpy: self.score_usd_jpy(regime, inputs, risk_score),
            wti_oil: self.score_wti_oil(regime, inputs, liquidity_score),
            natural_gas: self.score_natural_gas(inputs),
        }
    }
    
    /// Score Gold (XAU/USD)
    /// Key drivers: Real yields, DXY, Risk sentiment, VIX
    fn score_gold(&self, regime: Regime, inputs: &MrateInputs, liquidity_score: f64, risk_score: f64) -> InstrumentScore {
        let mut score = 50.0;
        let mut factors = Vec::new();
        
        // Regime bonus (0-15 pts)
        match regime {
            Regime::GoldSuperBull => { score += 15.0; factors.push("Gold Super Bull regime".into()); }
            Regime::Panic => { score += 12.0; factors.push("Safe haven demand in panic".into()); }
            Regime::Trend => { score += 5.0; }
            Regime::BtcSuperBull => { score -= 5.0; factors.push("Risk-on favors crypto".into()); }
            Regime::Choppy => { score -= 3.0; }
        }
        
        // Real yields - THE key driver (-1% = +20pts, +2% = -15pts)
        if let Some(real_yield) = inputs.real_yield_10y {
            let yield_factor = (1.0 - real_yield) * 10.0; // -1% = +20, 0% = +10, +2% = -10
            score += yield_factor.clamp(-15.0, 20.0);
            if real_yield < 0.5 {
                factors.push(format!("Low real yields ({:.2}%)", real_yield));
            } else if real_yield > 1.5 {
                factors.push(format!("High real yields ({:.2}%) headwind", real_yield));
            }
        } else {
            score -= 10.0; // Penalize missing critical data
            factors.push("Missing real yield data".into());
        }
        
        // DXY (weak dollar = bullish gold)
        if let Some(dxy) = inputs.dxy_level {
            if dxy < 100.0 {
                score += (100.0 - dxy) * 0.5; // DXY 95 = +2.5pts
                if dxy < 98.0 { factors.push("Weak dollar supportive".into()); }
            } else {
                score -= (dxy - 100.0) * 0.5;
                if dxy > 104.0 { factors.push("Strong dollar headwind".into()); }
            }
        }
        
        // Risk sentiment (high fear = bullish)
        score += (risk_score - 50.0) * 0.15;
        
        // VIX (elevated VIX = gold demand)
        if let Some(vix) = inputs.vix_level {
            if vix > 25.0 {
                score += (vix - 25.0) * 0.3;
                factors.push(format!("Elevated VIX ({:.1})", vix));
            }
        }
        
        // Volatility penalty (high ATR = harder to trade)
        if let Some(atr) = inputs.atr_percentile {
            if atr > 75.0 {
                score -= (atr - 75.0) * 0.3;
                factors.push("High volatility".into());
            }
        }
        
        // GLD ETF volume (institutional flow proxy)
        if let Some(vol_ratio) = inputs.gold_etf_flow_weekly {
            if vol_ratio > 1.5 {
                score += 5.0;
                factors.push(format!("Heavy GLD volume ({:.1}x avg) — institutional interest", vol_ratio));
            } else if vol_ratio < 0.5 {
                score -= 3.0;
                factors.push("Low GLD volume — weak institutional interest".into());
            }
        }
        
        // Reddit gold sentiment (if mentioned in WSB/investing communities)
        if let Some(reddit_gold) = inputs.reddit_gold_sentiment {
            score += (reddit_gold * 3.0).clamp(-5.0, 5.0);
            if reddit_gold > 0.3 {
                factors.push(format!("Retail gold interest ({:.2})", reddit_gold));
            }
        }
        
        // Gold-DXY correlation breakdown detection
        // Gold normally moves INVERSELY to DXY. If both trend the same way, beware.
        if let (Some(dxy_trend), Some(dxy)) = (inputs.dxy_trend, inputs.dxy_level) {
            let dxy_bullish = matches!(dxy_trend, TrendDirection::Up | TrendDirection::StrongUp);
            let gold_bullish = score > 55.0; // Our own score so far suggests gold is bullish
            
            if dxy_bullish && gold_bullish && dxy > 101.0 {
                // DXY is trending up AND gold looks bullish — unusual correlation
                score -= 5.0;
                factors.push("⚠️ Gold-DXY correlation breakdown (both bullish)".into());
            }
            
            let dxy_bearish = matches!(dxy_trend, TrendDirection::Down | TrendDirection::StrongDown);
            let gold_bearish = score < 45.0;
            if dxy_bearish && gold_bearish && dxy < 99.0 {
                // DXY falling AND gold also bearish — unusual
                score -= 3.0;
                factors.push("⚠️ Gold-DXY correlation breakdown (both bearish)".into());
            }
        }
        
        let score = score.clamp(0.0, 100.0);
        
        // Get trend direction and ADX for price regime
        let adx_value = inputs.gold_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        
        // Use technicals for richer direction detection
        let technicals = inputs.gold_technicals.clone().unwrap_or_default();
        
        // Determine trend direction from technicals (+DI/-DI is more accurate than DXY inverse)
        let trend_direction = if technicals.plus_di > 0.0 || technicals.minus_di > 0.0 {
            if technicals.plus_di > technicals.minus_di + 5.0 {
                if technicals.plus_di > technicals.minus_di + 15.0 { TrendDirection::StrongUp } else { TrendDirection::Up }
            } else if technicals.minus_di > technicals.plus_di + 5.0 {
                if technicals.minus_di > technicals.plus_di + 15.0 { TrendDirection::StrongDown } else { TrendDirection::Down }
            } else {
                TrendDirection::Flat
            }
        } else {
            // Fallback to DXY inverse
            inputs.dxy_trend.map(|t| t.inverse()).unwrap_or(TrendDirection::Flat)
        };
        
        // Add technical factors
        if technicals.rsi > 0.0 {
            if technicals.rsi > 70.0 { factors.push(format!("RSI overbought ({:.0})", technicals.rsi)); }
            else if technicals.rsi < 30.0 { factors.push(format!("RSI oversold ({:.0})", technicals.rsi)); }
        }
        if technicals.momentum_score > 60.0 {
            factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score));
        } else if technicals.momentum_score < 40.0 {
            factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score));
        }
        
        // Derive trading direction from MTF consensus (D1=50%, H1=30%, M15=20%)
        let trading_direction = derive_direction_mtf(
            inputs.gold_technicals_d1.as_ref(),
            inputs.gold_technicals_h1.as_ref(),
            inputs.gold_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        // Build Multi-Timeframe Confluence data
        let mtf_data = self.build_mtf_data(
            score,
            inputs.gold_technicals.as_ref(),
            inputs.gold_adx,
            inputs.gold_technicals_h1.as_ref(),
            inputs.gold_adx_h1,
            inputs.gold_technicals_m15.as_ref(),
            inputs.gold_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        let (d1_atr, stop_atr_multiplier, recommended_stop_distance, recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.gold_technicals_d1.as_ref(), price_regime);
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.gold_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "XAU_USD".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Score Bitcoin (BTC/USD)
    /// Key drivers: Liquidity, BTC sentiment, Risk-on/off, Stablecoin dominance
    fn score_bitcoin(&self, regime: Regime, inputs: &MrateInputs, liquidity_score: f64, risk_score: f64) -> InstrumentScore {
        let mut score = 50.0;
        let mut factors = Vec::new();
        
        // Regime bonus
        match regime {
            Regime::BtcSuperBull => { score += 20.0; factors.push("BTC Super Bull regime".into()); }
            Regime::Trend => { score += 8.0; factors.push("Trending markets favor momentum".into()); }
            Regime::GoldSuperBull => { score += 3.0; } // Still some spillover
            Regime::Panic => { score -= 15.0; factors.push("Risk-off hurts crypto".into()); }
            Regime::Choppy => { score -= 8.0; factors.push("Choppy markets difficult for BTC".into()); }
        }
        
        // Liquidity (high liquidity = bullish BTC)
        score += (liquidity_score - 50.0) * 0.25;
        if liquidity_score > 70.0 {
            factors.push("Strong liquidity conditions".into());
        }
        
        // BTC sentiment from Polymarket
        if let Some(btc_sent) = inputs.btc_sentiment {
            score += (btc_sent - 50.0) * 0.2;
            if btc_sent > 65.0 {
                factors.push(format!("Bullish sentiment ({:.0}%)", btc_sent));
            } else if btc_sent < 35.0 {
                factors.push(format!("Bearish sentiment ({:.0}%)", btc_sent));
            }
        }
        
        // Reddit BTC sentiment (additional social signal)
        if let Some(reddit_btc) = inputs.reddit_btc_sentiment {
            score += (reddit_btc * 5.0).clamp(-10.0, 10.0);
            if reddit_btc > 0.3 {
                factors.push(format!("Reddit bullish ({:.2})", reddit_btc));
            } else if reddit_btc < -0.3 {
                factors.push(format!("Reddit bearish ({:.2})", reddit_btc));
            }
        }
        
        // Reddit dispersion penalty (conflicting social sentiment = uncertainty)
        if let Some(dispersion) = inputs.reddit_dispersion {
            if dispersion > 0.5 {
                score -= (dispersion - 0.5) * 10.0;
                factors.push("High Reddit sentiment dispersion".into());
            }
        }
        
        // Risk-on/off (low risk score = bullish)
        score += (50.0 - risk_score) * 0.15;
        
        // Stablecoin dominance (high = dry powder ready)
        if let Some(stable) = inputs.stablecoin_dominance {
            if stable > 6.5 {
                score += (stable - 6.5) * 3.0;
                factors.push("High stablecoin reserves".into());
            }
        }
        
        // BTC trend bonus
        if let Some(trend) = inputs.btc_trend {
            match trend {
                TrendDirection::StrongUp => { score += 8.0; factors.push("Strong uptrend".into()); }
                TrendDirection::Up => { score += 4.0; }
                TrendDirection::Down => { score -= 4.0; }
                TrendDirection::StrongDown => { score -= 8.0; factors.push("Strong downtrend".into()); }
                _ => {}
            }
        }
        
        // Volatility penalty
        if let Some(atr) = inputs.btc_atr_percentile {
            if atr > 80.0 {
                score -= (atr - 80.0) * 0.4;
                factors.push("Extreme volatility".into());
            }
        }
        
        // Fear & Greed Index — crypto-native sentiment (±10 pts)
        if let Some(fg) = inputs.fear_greed_index {
            if fg > 75.0 {
                score += 10.0;
                factors.push(format!("Extreme Greed ({:.0}) — bullish crypto", fg));
            } else if fg > 60.0 {
                score += 5.0;
            } else if fg < 25.0 {
                score -= 10.0;
                factors.push(format!("Extreme Fear ({:.0}) — bearish crypto", fg));
            } else if fg < 40.0 {
                score -= 5.0;
            }
        }
        
        let score = score.clamp(0.0, 100.0);
        
        let trend_direction = inputs.btc_trend.unwrap_or(TrendDirection::Flat);
        let adx_value = inputs.btc_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        let technicals = inputs.btc_technicals.clone().unwrap_or_default();
        
        if technicals.momentum_score > 60.0 { factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score)); }
        else if technicals.momentum_score < 40.0 { factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score)); }
        
        let trading_direction = derive_direction_mtf(
            inputs.btc_technicals_d1.as_ref(),
            inputs.btc_technicals_h1.as_ref(),
            inputs.btc_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        let mtf_data = self.build_mtf_data(
            score,
            inputs.btc_technicals.as_ref(),
            inputs.btc_adx,
            inputs.btc_technicals_h1.as_ref(),
            inputs.btc_adx_h1,
            inputs.btc_technicals_m15.as_ref(),
            inputs.btc_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        let (d1_atr, stop_atr_multiplier, recommended_stop_distance, recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.btc_technicals_d1.as_ref(), price_regime);
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.btc_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "BTC_USD".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Score EUR/USD
    /// Key drivers: DXY (inverse), EUR/USD trend, Fed vs ECB expectations
    fn score_eur_usd(&self, regime: Regime, inputs: &MrateInputs) -> InstrumentScore {
        let mut score = 50.0;
        let mut factors = Vec::new();
        
        // DXY is the primary driver (inverse correlation) - STRONGER impact
        if let Some(dxy) = inputs.dxy_level {
            // DXY 100 = neutral, 95 = EUR bullish (+12.5), 105 = EUR bearish (-12.5)
            let dxy_factor = (100.0 - dxy) * 2.5;
            score += dxy_factor.clamp(-18.0, 18.0);
            if dxy < 98.0 {
                factors.push(format!("Weak DXY ({:.1}) bullish EUR", dxy));
            } else if dxy > 103.0 {
                factors.push(format!("Strong DXY ({:.1}) bearish EUR", dxy));
            }
        }
        
        // EUR/USD trend
        if let Some(trend) = inputs.eur_usd_trend {
            match trend {
                TrendDirection::StrongUp => { score += 12.0; factors.push("Strong EUR uptrend".into()); }
                TrendDirection::Up => { score += 6.0; factors.push("EUR uptrend".into()); }
                TrendDirection::Down => { score -= 6.0; }
                TrendDirection::StrongDown => { score -= 12.0; factors.push("Strong EUR downtrend".into()); }
                _ => {}
            }
        }
        
        // Fed rate expectations (high rate cut prob = USD weakness = EUR bullish)
        if let Some(rate_cut) = inputs.rate_cut_probability {
            if rate_cut > 60.0 {
                score += 5.0;
                factors.push("Fed dovish expectations".into());
            } else if rate_cut < 30.0 {
                score -= 5.0;
                factors.push("Fed hawkish".into());
            }
        }
        
        // Volatility (forex likes moderate vol)
        if let Some(atr) = inputs.eur_usd_atr_percentile {
            if atr > 75.0 {
                score -= (atr - 75.0) * 0.15;
            } else if atr < 20.0 {
                score -= 2.0; // Too quiet = no opportunity
            }
        }
        
        // Regime considerations - FX is ALWAYS tradeable, less regime dependent
        match regime {
            Regime::Panic => { score -= 3.0; factors.push("USD strength in panic".into()); }
            Regime::Trend => { score += 5.0; factors.push("Trending regime good for FX".into()); }
            Regime::Choppy => { score += 2.0; } // FX still works in choppy - high liquidity
            _ => {}
        }
        
        let score = score.clamp(0.0, 100.0);
        
        let trend_direction = inputs.eur_usd_trend.unwrap_or(TrendDirection::Flat);
        let adx_value = inputs.eur_usd_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        let technicals = inputs.eur_usd_technicals.clone().unwrap_or_default();
        
        if technicals.momentum_score > 60.0 { factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score)); }
        else if technicals.momentum_score < 40.0 { factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score)); }
        
        let trading_direction = derive_direction_mtf(
            inputs.eur_usd_technicals_d1.as_ref(),
            inputs.eur_usd_technicals_h1.as_ref(),
            inputs.eur_usd_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        let mtf_data = self.build_mtf_data(
            score,
            inputs.eur_usd_technicals.as_ref(),
            inputs.eur_usd_adx,
            inputs.eur_usd_technicals_h1.as_ref(),
            inputs.eur_usd_adx_h1,
            inputs.eur_usd_technicals_m15.as_ref(),
            inputs.eur_usd_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        let (d1_atr, stop_atr_multiplier, recommended_stop_distance, recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.eur_usd_technicals_d1.as_ref(), price_regime);
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.eur_usd_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "EUR_USD".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Score USD/JPY
    /// Key drivers: Rate differential, Risk sentiment (yen as safe haven), Carry trade viability
    fn score_usd_jpy(&self, regime: Regime, inputs: &MrateInputs, risk_score: f64) -> InstrumentScore {
        let mut score = 50.0;
        let mut factors = Vec::new();
        
        // US yields (higher = bullish USD/JPY for carry)
        if let Some(yield_10y) = inputs.yield_10y {
            if yield_10y > 4.5 {
                score += (yield_10y - 4.5) * 8.0;
                factors.push(format!("High US yields ({:.2}%) favor carry", yield_10y));
            } else if yield_10y < 3.5 {
                score -= (3.5 - yield_10y) * 5.0;
                factors.push("Low US yields reduce carry appeal".into());
            }
        }
        
        // Risk sentiment (risk-off = yen strength = bearish USD/JPY)
        if risk_score > 65.0 {
            score -= (risk_score - 65.0) * 0.4;
            factors.push("Risk-off yen demand".into());
        } else if risk_score < 35.0 {
            score += (35.0 - risk_score) * 0.3;
            factors.push("Risk-on reduces yen demand".into());
        }
        
        // USD/JPY trend
        if let Some(trend) = inputs.usd_jpy_trend {
            match trend {
                TrendDirection::StrongUp => { score += 8.0; factors.push("Strong uptrend".into()); }
                TrendDirection::Up => { score += 4.0; }
                TrendDirection::Down => { score -= 4.0; }
                TrendDirection::StrongDown => { score -= 8.0; factors.push("Strong downtrend".into()); }
                _ => {}
            }
        }
        
        // VIX impact (high VIX = yen strength typically)
        if let Some(vix) = inputs.vix_level {
            if vix > 25.0 {
                score -= (vix - 25.0) * 0.3;
            }
        }
        
        // Volatility
        if let Some(atr) = inputs.usd_jpy_atr_percentile {
            if atr > 75.0 {
                score -= (atr - 75.0) * 0.25;
                factors.push("High JPY volatility".into());
            }
        }
        
        // Regime
        match regime {
            Regime::Panic => { score -= 10.0; factors.push("Panic = yen strength".into()); }
            Regime::BtcSuperBull => { score += 5.0; factors.push("Risk-on supports carry".into()); }
            _ => {}
        }
        
        let score = score.clamp(0.0, 100.0);
        
        let trend_direction = inputs.usd_jpy_trend.unwrap_or(TrendDirection::Flat);
        let adx_value = inputs.usd_jpy_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        let technicals = inputs.usd_jpy_technicals.clone().unwrap_or_default();
        
        if technicals.momentum_score > 60.0 { factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score)); }
        else if technicals.momentum_score < 40.0 { factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score)); }
        
        let trading_direction = derive_direction_mtf(
            inputs.usd_jpy_technicals_d1.as_ref(),
            inputs.usd_jpy_technicals_h1.as_ref(),
            inputs.usd_jpy_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        let mtf_data = self.build_mtf_data(
            score,
            inputs.usd_jpy_technicals.as_ref(),
            inputs.usd_jpy_adx,
            inputs.usd_jpy_technicals_h1.as_ref(),
            inputs.usd_jpy_adx_h1,
            inputs.usd_jpy_technicals_m15.as_ref(),
            inputs.usd_jpy_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        let (d1_atr, stop_atr_multiplier, recommended_stop_distance, recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.usd_jpy_technicals_d1.as_ref(), price_regime);
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.usd_jpy_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "USD_JPY".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Score WTI Oil
    /// Key drivers: Economic demand (S&P), DXY, Risk appetite
    fn score_wti_oil(&self, regime: Regime, inputs: &MrateInputs, liquidity_score: f64) -> InstrumentScore {
        let mut score = 50.0;
        let mut factors = Vec::new();
        
        // Economic demand proxy (S&P trend)
        if let Some(sp_trend) = inputs.sp500_trend {
            match sp_trend {
                TrendDirection::StrongUp => { score += 12.0; factors.push("Strong economic demand".into()); }
                TrendDirection::Up => { score += 6.0; }
                TrendDirection::Down => { score -= 6.0; }
                TrendDirection::StrongDown => { score -= 12.0; factors.push("Weak economic demand".into()); }
                _ => {}
            }
        }
        
        // DXY (weak dollar = bullish oil) - STRONGER impact
        if let Some(dxy) = inputs.dxy_level {
            let dxy_factor = (100.0 - dxy) * 1.5;
            score += dxy_factor.clamp(-12.0, 12.0);
            if dxy < 98.0 {
                factors.push("Weak dollar bullish oil".into());
            } else if dxy > 103.0 {
                factors.push("Strong dollar headwind".into());
            }
        }
        
        // Liquidity (expansion = bullish commodities)
        score += (liquidity_score - 50.0) * 0.15;
        
        // Oil trend
        if let Some(trend) = inputs.oil_trend {
            match trend {
                TrendDirection::StrongUp => { score += 10.0; factors.push("Strong oil uptrend".into()); }
                TrendDirection::Up => { score += 5.0; }
                TrendDirection::Down => { score -= 5.0; }
                TrendDirection::StrongDown => { score -= 10.0; factors.push("Oil in downtrend".into()); }
                _ => {}
            }
        }
        
        // Volatility
        if let Some(atr) = inputs.oil_atr_percentile {
            if atr > 70.0 {
                score -= (atr - 70.0) * 0.25;
                factors.push("High oil volatility".into());
            }
        }
        
        // Regime
        match regime {
            Regime::Panic => { score -= 8.0; factors.push("Demand destruction fears".into()); }
            Regime::Trend => { score += 5.0; factors.push("Trending regime good for oil".into()); }
            _ => {}
        }
        
        let score = score.clamp(0.0, 100.0);
        
        let trend_direction = inputs.oil_trend.unwrap_or(TrendDirection::Flat);
        let adx_value = inputs.oil_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        let technicals = inputs.oil_technicals.clone().unwrap_or_default();
        
        if technicals.momentum_score > 60.0 { factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score)); }
        else if technicals.momentum_score < 40.0 { factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score)); }
        
        let trading_direction = derive_direction_mtf(
            inputs.oil_technicals_d1.as_ref(),
            inputs.oil_technicals_h1.as_ref(),
            inputs.oil_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        let mtf_data = self.build_mtf_data(
            score,
            inputs.oil_technicals.as_ref(),
            inputs.oil_adx,
            inputs.oil_technicals_h1.as_ref(),
            inputs.oil_adx_h1,
            inputs.oil_technicals_m15.as_ref(),
            inputs.oil_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        let (d1_atr, stop_atr_multiplier, recommended_stop_distance, recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.oil_technicals_d1.as_ref(), price_regime);
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.oil_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "WTICO_USD".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Score Natural Gas
    /// Key drivers: Trend, Volatility (notoriously volatile), Avoid in extreme conditions
    /// NOTE: This is a DANGEROUS instrument - scoring is intentionally conservative
    fn score_natural_gas(&self, inputs: &MrateInputs) -> InstrumentScore {
        let mut score = 35.0; // Start LOW - this is a dangerous instrument
        let mut factors = Vec::new();
        
        factors.push("⚠️ High-risk instrument".into());
        
        // Natural gas is extremely volatile - volatility is THE KEY factor
        if let Some(atr) = inputs.natgas_atr_percentile {
            if atr > 70.0 {
                score -= (atr - 70.0) * 1.0;
                factors.push(format!("High volatility ({:.0}%ile) - dangerous", atr));
            } else if atr > 50.0 {
                score -= (atr - 50.0) * 0.3;
                factors.push("Elevated volatility".into());
            } else if atr < 35.0 {
                score += 15.0;
                factors.push("Unusually calm - rare opportunity".into());
            }
        } else {
            score -= 10.0; // No volatility data = don't trade
            factors.push("No volatility data - avoid".into());
        }
        
        // Trend (ONLY trade with STRONG trend in natgas)
        if let Some(trend) = inputs.natgas_trend {
            match trend {
                TrendDirection::StrongUp => { 
                    score += 20.0; 
                    factors.push("Strong uptrend - momentum play".into()); 
                }
                TrendDirection::StrongDown => { 
                    score += 18.0; 
                    factors.push("Strong downtrend - momentum play".into()); 
                }
                TrendDirection::Up | TrendDirection::Down => { 
                    score += 5.0; 
                    // Weak trend in natgas is not enough
                }
                TrendDirection::Flat => { 
                    score -= 15.0;
                    factors.push("No trend - definitely avoid".into()); 
                }
            }
        } else {
            score -= 10.0;
            factors.push("Trend data unavailable - avoid".into());
        }
        
        // VIX impact (high VIX = absolutely avoid volatile instruments)
        if let Some(vix) = inputs.vix_level {
            if vix > 25.0 {
                score -= (vix - 25.0) * 0.8;
                factors.push("Elevated VIX - avoid natgas".into());
            }
        }
        
        let score = score.clamp(0.0, 100.0);
        
        let trend_direction = inputs.natgas_trend.unwrap_or(TrendDirection::Flat);
        let adx_value = inputs.natgas_adx.unwrap_or(20.0);
        let price_regime = PriceRegime::from_adx(adx_value);
        let technicals = inputs.natgas_technicals.clone().unwrap_or_default();
        
        if technicals.momentum_score > 60.0 { factors.push(format!("Bullish momentum ({:.0})", technicals.momentum_score)); }
        else if technicals.momentum_score < 40.0 { factors.push(format!("Bearish momentum ({:.0})", technicals.momentum_score)); }
        
        let trading_direction = derive_direction_mtf(
            inputs.natgas_technicals_d1.as_ref(),
            inputs.natgas_technicals_h1.as_ref(),
            inputs.natgas_technicals_m15.as_ref(),
            score,
        );
        let best_strategies = technicals.best_strategy_types(price_regime);
        
        let mtf_data = self.build_mtf_data(
            score,
            inputs.natgas_technicals.as_ref(),
            inputs.natgas_adx,
            inputs.natgas_technicals_h1.as_ref(),
            inputs.natgas_adx_h1,
            inputs.natgas_technicals_m15.as_ref(),
            inputs.natgas_adx_m15,
        );
        
        // Calculate ATR-based stop recommendations from D1
        // Note: Natural gas gets wider stops due to extreme volatility
        let (d1_atr, mut stop_atr_multiplier, mut recommended_stop_distance, mut recommended_tp_distance) = 
            self.calculate_atr_stops(inputs.natgas_technicals_d1.as_ref(), price_regime);
        
        // Natural gas override: minimum 3x ATR due to extreme volatility
        if stop_atr_multiplier < 3.0 && d1_atr > 0.0 {
            stop_atr_multiplier = 3.0;
            recommended_stop_distance = d1_atr * 3.0;
            recommended_tp_distance = recommended_stop_distance * 1.5;
        }
        
        // Extract D1 EMA values for UI display
        let (ema_200, current_price) = inputs.natgas_technicals_d1.as_ref()
            .map(|d1| (d1.ema_200, d1.current_price))
            .unwrap_or((0.0, 0.0));
        
        InstrumentScore {
            symbol: "NATGAS_USD".into(),
            score,
            recommendation: InstrumentRecommendation::from_score(score),
            factors,
            trend_direction,
            trend_strength: adx_value,
            price_regime,
            technicals,
            trading_direction,
            best_strategies,
            mtf_data,
            d1_atr,
            recommended_stop_distance,
            recommended_tp_distance,
            stop_atr_multiplier,
            ema_200,
            current_price,
        }
    }
    
    /// Build multi-timeframe confluence data
    fn build_mtf_data(
        &self,
        macro_score: f64,
        htf_technicals: Option<&InstrumentTechnicals>,
        htf_adx: Option<f64>,
        mtf_technicals: Option<&InstrumentTechnicals>,
        mtf_adx: Option<f64>,
        ltf_technicals: Option<&InstrumentTechnicals>,
        ltf_adx: Option<f64>,
    ) -> Option<MultiTimeframeData> {
        // Need at least HTF data to calculate MTF confluence
        let htf_tech = htf_technicals?;
        let htf_adx_val = htf_adx.unwrap_or(20.0);
        
        let htf = TimeframeTechnicals::from_technicals("H4", htf_tech.clone(), htf_adx_val, macro_score);
        
        // MTF (H1) - use HTF as fallback if not available
        let mtf = if let Some(tech) = mtf_technicals {
            TimeframeTechnicals::from_technicals("H1", tech.clone(), mtf_adx.unwrap_or(20.0), macro_score)
        } else {
            // Clone HTF but label as H1 (degraded mode)
            let mut fallback = htf.clone();
            fallback.timeframe = "H1".to_string();
            fallback
        };
        
        // LTF (M15) - use MTF as fallback if not available
        let ltf = if let Some(tech) = ltf_technicals {
            TimeframeTechnicals::from_technicals("M15", tech.clone(), ltf_adx.unwrap_or(20.0), macro_score)
        } else {
            // Clone MTF but label as M15 (degraded mode)
            let mut fallback = mtf.clone();
            fallback.timeframe = "M15".to_string();
            fallback
        };
        
        Some(MultiTimeframeData::calculate(htf, mtf, ltf))
    }
    
    /// Calculate ATR-based stop recommendations from D1 timeframe
    /// Returns (d1_atr, stop_multiplier, stop_distance, tp_distance)
    fn calculate_atr_stops(
        &self,
        d1_technicals: Option<&InstrumentTechnicals>,
        price_regime: PriceRegime,
    ) -> (f64, f64, f64, f64) {
        // Get D1 ATR (true daily volatility)
        let d1_atr = d1_technicals.map(|t| t.atr).unwrap_or(0.0);
        
        if d1_atr <= 0.0 {
            // No ATR data - return zeros, bot will use its own calculation
            return (0.0, 2.0, 0.0, 0.0);
        }
        
        // Base multiplier is 2x ATR for stops (gives room to breathe)
        // Adjust based on price regime:
        // - Trending: tighter stops (1.5x) - trend should protect you
        // - Ranging: wider stops (2.5x) - need room for mean reversion
        // - Transitional: standard (2.0x)
        let stop_mult = match price_regime {
            PriceRegime::Trending => 1.5,
            PriceRegime::Ranging => 2.5,
            PriceRegime::Transitional => 2.0,
        };
        
        let stop_distance = d1_atr * stop_mult;
        
        // TP is 1.5x the stop distance (1.5:1 R:R minimum)
        let tp_distance = stop_distance * 1.5;
        
        (d1_atr, stop_mult, stop_distance, tp_distance)
    }
    
    /// Determine which instrument is favored
    /// Note: If real yield data is unavailable, gold recommendations are downgraded
    fn determine_favored_instrument(&self, regime: Regime, inputs: &MrateInputs) -> FavoredInstrument {
        // Check if we have real yield data - critical for gold decisions
        let has_real_yield = inputs.real_yield_10y.is_some();
        
        let base_favored = match regime {
            Regime::GoldSuperBull => {
                // Gold is strongly favored in risk-off + liquidity expansion
                // But if BTC sentiment is also high, both could work
                if inputs.btc_sentiment.unwrap_or(0.0) > 60.0 {
                    FavoredInstrument::Both
                } else {
                    FavoredInstrument::Gold
                }
            }
            Regime::BtcSuperBull => {
                // Risk-on environment strongly favors BTC
                FavoredInstrument::Bitcoin
            }
            Regime::Panic => {
                // In panic, gold as safe haven
                FavoredInstrument::Gold
            }
            Regime::Trend => {
                // Trending market - check individual instrument trends
                let gold_trend_up = inputs.atr_percentile.unwrap_or(50.0) < 70.0; // Not too volatile
                let btc_trend_up = matches!(
                    inputs.btc_trend,
                    Some(TrendDirection::Up) | Some(TrendDirection::StrongUp)
                );
                let btc_not_volatile = inputs.btc_atr_percentile.unwrap_or(50.0) < 80.0;
                
                match (gold_trend_up, btc_trend_up && btc_not_volatile) {
                    (true, true) => FavoredInstrument::Both,
                    (true, false) => FavoredInstrument::Gold,
                    (false, true) => FavoredInstrument::Bitcoin,
                    (false, false) => FavoredInstrument::Neither,
                }
            }
            Regime::Choppy => {
                // Choppy market - be selective
                // Only favor if volatility is low enough for mean reversion
                let gold_ok = inputs.atr_percentile.unwrap_or(50.0) < 60.0;
                let btc_ok = inputs.btc_atr_percentile.unwrap_or(50.0) < 50.0; // BTC needs lower vol for MR
                
                match (gold_ok, btc_ok) {
                    (true, true) => FavoredInstrument::Both,
                    (true, false) => FavoredInstrument::Gold,
                    (false, true) => FavoredInstrument::Bitcoin,
                    (false, false) => FavoredInstrument::Neither,
                }
            }
        };
        
        // Downgrade gold recommendations if real yield data is unavailable
        // Real yield is THE key driver for gold - without it, we're flying blind
        if !has_real_yield {
            match base_favored {
                FavoredInstrument::Gold => {
                    tracing::warn!("Gold favored but real yield data unavailable - downgrading to Neither");
                    FavoredInstrument::Neither
                }
                FavoredInstrument::Both => {
                    tracing::warn!("Gold+BTC favored but real yield data unavailable - downgrading to Bitcoin only");
                    FavoredInstrument::Bitcoin
                }
                other => other,
            }
        } else {
            base_favored
        }
    }
    
    /// Calculate trend direction from candles
    fn calculate_trend(&self, candles: &[Candle]) -> TrendDirection {
        if candles.len() < 10 {
            return TrendDirection::Flat;
        }
        
        // Compare current price to 10-period and 20-period ago
        let current = candles.last().map(|c| c.close).unwrap_or(0.0);
        let mid = candles.get(candles.len().saturating_sub(10)).map(|c| c.close).unwrap_or(current);
        let start = candles.first().map(|c| c.close).unwrap_or(current);
        
        if start == 0.0 || current == 0.0 {
            return TrendDirection::Flat;
        }
        
        let short_change = (current - mid) / mid * 100.0;
        let long_change = (current - start) / start * 100.0;
        
        // Strong trend: both short and long term aligned and > 2%
        if long_change > 2.0 && short_change > 1.0 {
            TrendDirection::StrongUp
        } else if long_change < -2.0 && short_change < -1.0 {
            TrendDirection::StrongDown
        } else if long_change > 1.0 {
            TrendDirection::Up
        } else if long_change < -1.0 {
            TrendDirection::Down
        } else {
            TrendDirection::Flat
        }
    }
    
    /// Calculate ATR percentile vs historical average
    fn calculate_atr_percentile(&self, candles: &[Candle]) -> f64 {
        if candles.len() < 50 {
            return 50.0;
        }
        
        // Calculate ATR for each period
        let mut atrs: Vec<f64> = Vec::new();
        for i in 1..candles.len() {
            let high = candles[i].high;
            let low = candles[i].low;
            let prev_close = candles[i - 1].close;
            
            let tr = (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs());
            atrs.push(tr);
        }
        
        if atrs.is_empty() {
            return 50.0;
        }
        
        // Current ATR (14-period average of recent)
        let recent_count = 14.min(atrs.len());
        let current_atr: f64 = atrs[atrs.len() - recent_count..].iter().sum::<f64>() / recent_count as f64;
        
        // Historical ATR average
        let hist_atr: f64 = atrs.iter().sum::<f64>() / atrs.len() as f64;
        
        if hist_atr == 0.0 {
            return 50.0;
        }
        
        // Percentile: how does current compare to average
        // ATR at average = 50, 2x average = 100, 0.5x average = 25
        let ratio = current_atr / hist_atr;
        ((ratio - 0.5) / 1.5 * 100.0).clamp(0.0, 100.0)
    }
    
    /// Calculate ADX (Average Directional Index) for trend strength measurement
    /// Returns None if insufficient data, otherwise ADX value (0-100)
    fn calculate_adx(&self, candles: &[Candle]) -> Option<f64> {
        use crate::engine::indicators::adx;
        
        if candles.len() < 30 {
            return None;
        }
        
        // adx() returns (adx_values, plus_di, minus_di)
        let (adx_values, _, _) = adx(candles, 14);
        
        // Get the last ADX value
        adx_values.last().copied()
    }
    
    /// Calculate full technical profile for an instrument from its candles
    /// Returns InstrumentTechnicals with RSI, Bollinger, MACD, +DI/-DI, Stochastic, EMAs
    fn calculate_technicals(&self, candles: &[Candle]) -> Option<InstrumentTechnicals> {
        use crate::engine::indicators::{rsi, bollinger_bands, macd, adx, stochastic, ema, atr};
        
        if candles.len() < 50 {
            return None;
        }
        
        let n = candles.len();
        let last = n - 1;
        
        // RSI (14-period)
        let rsi_values = rsi(candles, 14);
        let rsi_val = rsi_values.get(last).copied().unwrap_or(50.0);
        
        // Bollinger Bands (20-period, 2.0 std dev)
        let (upper, _middle, lower) = bollinger_bands(candles, 20, 2.0);
        let bb_upper = upper.last().copied().unwrap_or(0.0);
        let bb_lower = lower.last().copied().unwrap_or(0.0);
        let current_price = candles[last].close;
        let bollinger_pct_b = if (bb_upper - bb_lower).abs() > 0.0001 {
            (current_price - bb_lower) / (bb_upper - bb_lower)
        } else {
            0.5
        };
        
        // MACD (12, 26, 9)
        let (_macd_line, _signal_line, histogram) = macd(candles, 12, 26, 9);
        let macd_hist = histogram.last().copied().unwrap_or(0.0);
        
        // ADX with +DI and -DI (14-period)
        let (adx_vals, plus_di_vals, minus_di_vals) = adx(candles, 14);
        let plus_di = plus_di_vals.last().copied().unwrap_or(0.0);
        let minus_di = minus_di_vals.last().copied().unwrap_or(0.0);
        
        // Stochastic (14, 3)
        let (k_values, d_values) = stochastic(candles, 14, 3);
        let stoch_k = k_values.last().copied().unwrap_or(50.0);
        let stoch_d = d_values.last().copied().unwrap_or(50.0);
        
        // EMA 50 and EMA 200
        let ema_50_vals = ema(candles, 50);
        let ema_200_vals = if n >= 200 { ema(candles, 200) } else { ema(candles, n.min(100)) };
        let ema_50 = ema_50_vals.last().copied().unwrap_or(current_price);
        let ema_200 = ema_200_vals.last().copied().unwrap_or(current_price);
        
        // ATR (14-period)
        let atr_values = atr(candles, 14);
        let atr_val = atr_values.last().copied().unwrap_or(0.0);
        
        // === Composite Momentum Score ===
        // Weighted combination of all indicators into 0-100 score
        // >60 = bullish momentum, <40 = bearish momentum
        let mut momentum = 50.0;
        
        // +DI vs -DI (weight: 25%)
        let di_ratio = if (plus_di + minus_di) > 0.0 {
            plus_di / (plus_di + minus_di) * 100.0  // 0-100, >50 = bullish
        } else { 50.0 };
        momentum += (di_ratio - 50.0) * 0.25;
        
        // RSI (weight: 20%) - normalize around 50
        momentum += (rsi_val - 50.0) * 0.20;
        
        // MACD histogram sign (weight: 20%)
        // Normalize MACD histogram relative to price
        let macd_normalized = if current_price > 0.0 {
            (macd_hist / current_price * 10000.0).clamp(-25.0, 25.0)  // Normalized to price
        } else { 0.0 };
        momentum += macd_normalized * 0.20;
        
        // EMA alignment (weight: 20%)
        if current_price > ema_50 && ema_50 > ema_200 { momentum += 10.0; }  // Golden cross
        else if current_price < ema_50 && ema_50 < ema_200 { momentum -= 10.0; }  // Death cross
        else if current_price > ema_50 { momentum += 5.0; }
        else if current_price < ema_50 { momentum -= 5.0; }
        
        // Stochastic (weight: 15%)
        momentum += (stoch_k - 50.0) * 0.15;
        
        let momentum_score = momentum.clamp(0.0, 100.0);
        
        Some(InstrumentTechnicals {
            rsi: rsi_val,
            bollinger_percent_b: bollinger_pct_b,
            macd_histogram: macd_hist,
            plus_di,
            minus_di,
            stochastic_k: stoch_k,
            stochastic_d: stoch_d,
            momentum_score,
            current_price,
            ema_50,
            ema_200,
            atr: atr_val,
        })
    }
}

impl Default for MrateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_regime_detection_with_buffer_zones() {
        let engine = MrateEngine::new();
        
        // High liquidity + High risk = Gold Super Bull (threshold: 73)
        assert_eq!(engine.calculate_raw_regime(80.0, 80.0), Regime::GoldSuperBull);
        
        // High liquidity + Low risk = BTC Super Bull (threshold: liq>73, risk<37)
        assert_eq!(engine.calculate_raw_regime(80.0, 30.0), Regime::BtcSuperBull);
        
        // Low liquidity + High risk = Panic (threshold: liq<37, risk>73)
        assert_eq!(engine.calculate_raw_regime(30.0, 80.0), Regime::Panic);
        
        // Moderate liquidity = Trend (entry threshold: 63)
        assert_eq!(engine.calculate_raw_regime(70.0, 50.0), Regime::Trend);
        
        // Low conviction = Choppy
        assert_eq!(engine.calculate_raw_regime(50.0, 50.0), Regime::Choppy);
        
        // Test buffer zone: 60 is in the "sticky" zone - depends on current regime
        // Default regime is Choppy, so 60 should stay Choppy (need >63 to enter Trend)
        assert_eq!(engine.calculate_raw_regime(60.0, 50.0), Regime::Choppy);
    }
    
    #[test]
    fn test_regime_hysteresis() {
        let engine = MrateEngine::new();
        
        // First reading of Trend - should not change yet (need 2 consecutive)
        let (regime1, pending1) = engine.apply_regime_hysteresis(Regime::Trend);
        assert_eq!(regime1, Regime::Choppy); // Still Choppy
        assert!(pending1); // Change is pending
        
        // Second consecutive Trend reading - should confirm change
        let (regime2, pending2) = engine.apply_regime_hysteresis(Regime::Trend);
        assert_eq!(regime2, Regime::Trend); // Now Trend
        assert!(!pending2); // No longer pending
        
        // Single Choppy reading - should not revert immediately
        let (regime3, pending3) = engine.apply_regime_hysteresis(Regime::Choppy);
        assert_eq!(regime3, Regime::Trend); // Still Trend
        assert!(pending3); // Revert is pending
    }
    
    #[test]
    fn test_risk_multiplier() {
        let engine = MrateEngine::new();
        
        // Low uncertainty = high multiplier
        assert!((engine.calculate_risk_multiplier(20.0) - 1.3).abs() < 0.01);
        
        // Medium uncertainty
        assert!((engine.calculate_risk_multiplier(50.0) - 1.0).abs() < 0.01);
        
        // High uncertainty = low multiplier
        assert!((engine.calculate_risk_multiplier(80.0) - 0.7).abs() < 0.01);
    }
}
