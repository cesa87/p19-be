//! MRATE Data Models
//! 
//! Core data structures for the Macro-Regime Adaptive Trading Engine

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use std::collections::VecDeque;

/// Market regime classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Regime {
    /// High liquidity + High risk = Gold safe haven rally
    GoldSuperBull,
    /// High liquidity + Low risk = Risk-on BTC rally
    BtcSuperBull,
    /// Low liquidity + High risk = Market panic
    Panic,
    /// Moderate liquidity expansion = Trending markets
    Trend,
    /// Low conviction = Choppy/ranging markets
    Choppy,
}

impl Regime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Regime::GoldSuperBull => "GOLD_SUPER_BULL",
            Regime::BtcSuperBull => "BTC_SUPER_BULL",
            Regime::Panic => "PANIC",
            Regime::Trend => "TREND",
            Regime::Choppy => "CHOPPY",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            Regime::GoldSuperBull => "Liquidity expanding + Risk-off sentiment. Gold as safe haven.",
            Regime::BtcSuperBull => "Liquidity expanding + Risk-on sentiment. BTC and risk assets rally.",
            Regime::Panic => "Liquidity tightening + High fear. Defensive positioning.",
            Regime::Trend => "Moderate liquidity. Clear directional trends.",
            Regime::Choppy => "Low conviction. Range-bound, mean reversion favored.",
        }
    }
    
    /// Check if this is a "strong" regime (Super Bull or Panic)
    pub fn is_strong(&self) -> bool {
        matches!(self, Regime::GoldSuperBull | Regime::BtcSuperBull | Regime::Panic)
    }
}

/// Strategy category for MRATE weighting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyCategory {
    Trend,
    Breakout,
    MeanReversion,
    LiquiditySweep,
}

impl StrategyCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyCategory::Trend => "trend",
            StrategyCategory::Breakout => "breakout",
            StrategyCategory::MeanReversion => "mean_reversion",
            StrategyCategory::LiquiditySweep => "liquidity_sweep",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trend" => Some(StrategyCategory::Trend),
            "breakout" => Some(StrategyCategory::Breakout),
            "mean_reversion" => Some(StrategyCategory::MeanReversion),
            "liquidity_sweep" => Some(StrategyCategory::LiquiditySweep),
            _ => None,
        }
    }
}

/// Strategy weights for each category (0.0 - 1.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyWeights {
    pub trend: f64,
    pub breakout: f64,
    pub mean_reversion: f64,
    pub liquidity_sweep: f64,
}

impl StrategyWeights {
    /// Get weight for a specific category
    pub fn get(&self, category: StrategyCategory) -> f64 {
        match category {
            StrategyCategory::Trend => self.trend,
            StrategyCategory::Breakout => self.breakout,
            StrategyCategory::MeanReversion => self.mean_reversion,
            StrategyCategory::LiquiditySweep => self.liquidity_sweep,
        }
    }
    
    /// Get weights for a specific regime
    /// Optimized for 26-bot fleet: 12 trend, 9 mean_reversion, 4 breakout, 1 liquidity_sweep
    pub fn for_regime(regime: Regime) -> Self {
        match regime {
            Regime::GoldSuperBull => Self {
                trend: 1.0,
                breakout: 0.85,          // was 0.9 - slight reduction
                mean_reversion: 0.45,    // was 0.2 - NOW TRADES (pullbacks in bull)
                liquidity_sweep: 0.35,   // was 0.3 - slight bump
            },
            Regime::BtcSuperBull => Self {
                trend: 1.0,
                breakout: 0.95,          // was 1.0 - tiny reduction
                mean_reversion: 0.40,    // was 0.1 - EDGE CASE (on threshold)
                liquidity_sweep: 0.25,   // was 0.2 - bump
            },
            Regime::Panic => Self {
                trend: 0.65,             // was 0.7 - slight reduction
                breakout: 0.45,          // was 0.3 - NOW TRADES (relief rallies)
                mean_reversion: 0.65,    // was 0.6 - bump (ranges tighten)
                liquidity_sweep: 0.9,    // no change - perfection
            },
            Regime::Trend => Self {
                trend: 0.85,             // was 0.9 - slight trim
                breakout: 0.7,           // no change
                mean_reversion: 0.50,    // was 0.3 - NOW TRADES STRONGLY (retracements)
                liquidity_sweep: 0.45,   // was 0.4 - bump
            },
            Regime::Choppy => Self {
                trend: 0.25,             // was 0.2 - tiny bump (lower TF trends)
                breakout: 0.35,          // was 0.2 - closer but still below threshold
                mean_reversion: 1.0,     // no change - MR paradise
                liquidity_sweep: 0.85,   // was 0.8 - bump (stop hunts common)
            },
        }
    }
}

/// Trend direction for market data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    StrongUp,
    Up,
    Flat,
    Down,
    StrongDown,
}

impl TrendDirection {
    /// Convert trend to score (0-100)
    /// For yields/DXY: down = bullish for liquidity
    pub fn to_inverse_score(&self) -> f64 {
        match self {
            TrendDirection::StrongUp => 0.0,
            TrendDirection::Up => 25.0,
            TrendDirection::Flat => 50.0,
            TrendDirection::Down => 75.0,
            TrendDirection::StrongDown => 100.0,
        }
    }
    
    /// Convert trend to score (0-100)
    /// For S&P: up = bullish for risk sentiment
    pub fn to_score(&self) -> f64 {
        match self {
            TrendDirection::StrongUp => 100.0,
            TrendDirection::Up => 75.0,
            TrendDirection::Flat => 50.0,
            TrendDirection::Down => 25.0,
            TrendDirection::StrongDown => 0.0,
        }
    }
    
    /// Get the inverse trend direction
    /// Useful for inversely correlated pairs (e.g., DXY vs Gold)
    pub fn inverse(&self) -> Self {
        match self {
            TrendDirection::StrongUp => TrendDirection::StrongDown,
            TrendDirection::Up => TrendDirection::Down,
            TrendDirection::Flat => TrendDirection::Flat,
            TrendDirection::Down => TrendDirection::Up,
            TrendDirection::StrongDown => TrendDirection::StrongUp,
        }
    }
}

impl Default for TrendDirection {
    fn default() -> Self {
        TrendDirection::Flat
    }
}

/// A historical score snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreSnapshot {
    pub liquidity: f64,
    pub risk: f64,
    pub timestamp: DateTime<Utc>,
}

/// Regime state for hysteresis tracking
/// Prevents regime flapping by requiring consecutive readings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeState {
    /// Current confirmed regime
    pub current_regime: Regime,
    /// Regime suggested by raw scores (may differ from current)
    pub proposed_regime: Regime,
    /// Number of consecutive readings suggesting the proposed regime
    pub consecutive_readings: u32,
    /// Timestamp of last regime change
    pub last_change: DateTime<Utc>,
    /// Required consecutive readings before regime change (default: 2)
    pub required_consecutive: u32,
    /// Rolling history of scores for momentum calculation (last ~12 readings)
    pub score_history: VecDeque<ScoreSnapshot>,
}

impl Default for RegimeState {
    fn default() -> Self {
        Self {
            current_regime: Regime::Choppy,
            proposed_regime: Regime::Choppy,
            consecutive_readings: 0,
            last_change: Utc::now(),
            required_consecutive: 2,
            score_history: VecDeque::with_capacity(12),
        }
    }
}

impl RegimeState {
    /// Update regime state with a new proposed regime
    /// Returns the confirmed regime (may or may not change)
    pub fn update(&mut self, proposed: Regime) -> Regime {
        if proposed == self.current_regime {
            // Same as current - reset counter
            self.proposed_regime = proposed;
            self.consecutive_readings = 0;
            return self.current_regime;
        }
        
        if proposed == self.proposed_regime {
            // Same as previous proposal - increment counter
            self.consecutive_readings += 1;
            
            if self.consecutive_readings >= self.required_consecutive {
                // Enough consecutive readings - confirm regime change
                let old_regime = self.current_regime;
                self.current_regime = proposed;
                self.last_change = Utc::now();
                self.consecutive_readings = 0;
                tracing::info!(
                    "Regime change confirmed: {:?} -> {:?}",
                    old_regime, proposed
                );
            }
        } else {
            // Different proposal - reset counter
            self.proposed_regime = proposed;
            self.consecutive_readings = 1;
        }
        
        self.current_regime
    }
    
    /// Check if a regime change is pending
    pub fn change_pending(&self) -> bool {
        self.proposed_regime != self.current_regime && self.consecutive_readings > 0
    }
    
    /// Minutes since last regime change
    pub fn minutes_since_change(&self) -> i64 {
        (Utc::now() - self.last_change).num_minutes()
    }
    
    /// Record a new score snapshot for momentum tracking
    pub fn record_scores(&mut self, liquidity: f64, risk: f64) {
        if self.score_history.len() >= 12 {
            self.score_history.pop_front();
        }
        self.score_history.push_back(ScoreSnapshot {
            liquidity,
            risk,
            timestamp: Utc::now(),
        });
    }
    
    /// Calculate liquidity momentum (positive = improving, negative = deteriorating)
    /// Compares recent 3 readings vs older 3 readings
    pub fn liquidity_momentum(&self) -> f64 {
        if self.score_history.len() < 6 {
            return 0.0;
        }
        let len = self.score_history.len();
        let recent_avg: f64 = self.score_history.iter().rev().take(3)
            .map(|s| s.liquidity).sum::<f64>() / 3.0;
        let older_avg: f64 = self.score_history.iter().skip(len.saturating_sub(6)).take(3)
            .map(|s| s.liquidity).sum::<f64>() / 3.0;
        recent_avg - older_avg
    }
    
    /// Calculate risk momentum (positive = risk increasing, negative = risk decreasing)
    pub fn risk_momentum(&self) -> f64 {
        if self.score_history.len() < 6 {
            return 0.0;
        }
        let len = self.score_history.len();
        let recent_avg: f64 = self.score_history.iter().rev().take(3)
            .map(|s| s.risk).sum::<f64>() / 3.0;
        let older_avg: f64 = self.score_history.iter().skip(len.saturating_sub(6)).take(3)
            .map(|s| s.risk).sum::<f64>() / 3.0;
        recent_avg - older_avg
    }
}

/// Data source health tracking
/// Tracks which feeds are working and when they last succeeded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataHealth {
    /// Polymarket API status
    pub polymarket_ok: bool,
    /// VIX feed status
    pub vix_ok: bool,
    /// FRED API status
    pub fred_ok: bool,
    /// OANDA API status
    pub oanda_ok: bool,
    /// Economic calendar status
    pub calendar_ok: bool,
    /// DXY feed status
    pub dxy_ok: bool,
    /// Fear & Greed Index feed status
    pub fear_greed_ok: bool,
    /// News sentiment feed status (Alpha Vantage)
    pub news_ok: bool,
    /// Reddit sentiment feed status
    pub reddit_ok: bool,
    
    /// Last successful Polymarket fetch
    pub last_polymarket_success: Option<DateTime<Utc>>,
    /// Last successful VIX fetch
    pub last_vix_success: Option<DateTime<Utc>>,
    /// Last successful FRED fetch
    pub last_fred_success: Option<DateTime<Utc>>,
    /// Last successful OANDA fetch
    pub last_oanda_success: Option<DateTime<Utc>>,
    /// Last successful DXY fetch
    pub last_dxy_success: Option<DateTime<Utc>>,
    /// Last successful Fear & Greed fetch
    pub last_fear_greed_success: Option<DateTime<Utc>>,
    /// Last successful news sentiment fetch
    pub last_news_success: Option<DateTime<Utc>>,
    /// Last successful Reddit fetch
    pub last_reddit_success: Option<DateTime<Utc>>,
    
    /// Minutes before data is considered stale
    pub stale_threshold_minutes: i64,
}

impl Default for DataHealth {
    fn default() -> Self {
        Self {
            polymarket_ok: false,
            vix_ok: false,
            fred_ok: false,
            oanda_ok: false,
            calendar_ok: false,
            dxy_ok: false,
            fear_greed_ok: false,
            news_ok: false,
            reddit_ok: false,
            last_polymarket_success: None,
            last_vix_success: None,
            last_fred_success: None,
            last_oanda_success: None,
            last_dxy_success: None,
            last_fear_greed_success: None,
            last_news_success: None,
            last_reddit_success: None,
            stale_threshold_minutes: 15,
        }
    }
}

impl DataHealth {
    /// Check if the system is in degraded mode (any critical source is down or stale)
    pub fn is_degraded(&self) -> bool {
        let now = Utc::now();
        let stale_threshold = Duration::minutes(self.stale_threshold_minutes);
        
        // Check if any source is currently failing
        if !self.polymarket_ok || !self.vix_ok {
            return true;
        }
        
        // Check for stale data
        if let Some(last) = self.last_polymarket_success {
            if now - last > stale_threshold {
                return true;
            }
        }
        
        if let Some(last) = self.last_vix_success {
            if now - last > stale_threshold {
                return true;
            }
        }
        
        false
    }
    
    /// Get a human-readable status summary
    pub fn status_summary(&self) -> String {
        let mut issues = Vec::new();
        
        if !self.polymarket_ok {
            issues.push("Polymarket DOWN");
        }
        if !self.vix_ok {
            issues.push("VIX DOWN");
        }
        if !self.fred_ok {
            issues.push("FRED DOWN");
        }
        if !self.oanda_ok {
            issues.push("OANDA DOWN");
        }
        if !self.calendar_ok {
            issues.push("Calendar DOWN");
        }
        if !self.dxy_ok {
            issues.push("DXY DOWN");
        }
        if !self.fear_greed_ok {
            issues.push("Fear & Greed DOWN");
        }
        if !self.news_ok {
            issues.push("News Sentiment DOWN");
        }
        if !self.reddit_ok {
            issues.push("Reddit Sentiment DOWN");
        }
        
        if issues.is_empty() {
            "All feeds healthy".to_string()
        } else {
            issues.join(", ")
        }
    }
    
    /// Count how many critical feeds are working
    pub fn healthy_feed_count(&self) -> usize {
        let mut count = 0;
        if self.polymarket_ok { count += 1; }
        if self.vix_ok { count += 1; }
        if self.fred_ok { count += 1; }
        if self.oanda_ok { count += 1; }
        if self.dxy_ok { count += 1; }
        if self.fear_greed_ok { count += 1; }
        if self.news_ok { count += 1; }
        if self.reddit_ok { count += 1; }
        count
    }
}

/// Raw input data for MRATE calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrateInputs {
    // Polymarket data
    pub rate_cut_probability: Option<f64>,
    pub inflation_expectations: Option<f64>,
    pub safe_haven_demand: Option<f64>,
    pub btc_sentiment: Option<f64>,
    pub fed_uncertainty: Option<f64>,
    
    // Market data trends
    pub dxy_trend: Option<TrendDirection>,
    pub sp500_trend: Option<TrendDirection>,
    pub yield_10y_trend: Option<TrendDirection>,
    
    // VIX (CBOE Volatility Index)
    pub vix_level: Option<f64>,
    
    // DXY (US Dollar Index)
    pub dxy_level: Option<f64>,
    
    // Fear & Greed Index (0-100, crypto sentiment)
    pub fear_greed_index: Option<f64>,
    pub fear_greed_category: Option<String>,
    
    // Treasury yields (from FRED)
    pub yield_10y: Option<f64>,       // 10-Year Treasury yield (%)
    pub yield_2y: Option<f64>,        // 2-Year Treasury yield (%)
    pub real_yield_10y: Option<f64>,  // 10-Year TIPS (real yield) (%)
    
    // Crypto market data
    pub stablecoin_dominance: Option<f64>,    // % of crypto market cap
    pub btc_exchange_netflow: Option<f64>,    // BTC flow to/from exchanges
    
    // Gold-specific
    pub atr_percentile: Option<f64>,
    pub gold_adx: Option<f64>,                // ADX value for trend strength
    pub gold_etf_flow_weekly: Option<f64>,    // GLD ETF flows (placeholder)
    pub high_impact_event_within_24h: bool,
    
    /// Hours until next high-impact event (NFP, CPI, FOMC, etc.)
    pub hours_to_next_high_impact: Option<f64>,
    /// Uncertainty weight multiplier of the nearest event (1.0-1.5)
    pub next_event_weight: f64,
    
    // BTC-specific data
    pub btc_trend: Option<TrendDirection>,
    pub btc_atr_percentile: Option<f64>,
    pub btc_adx: Option<f64>,
    
    // EUR/USD data
    pub eur_usd_trend: Option<TrendDirection>,
    pub eur_usd_atr_percentile: Option<f64>,
    pub eur_usd_adx: Option<f64>,
    
    // USD/JPY data
    pub usd_jpy_trend: Option<TrendDirection>,
    pub usd_jpy_atr_percentile: Option<f64>,
    pub usd_jpy_adx: Option<f64>,
    
    // WTI Oil data
    pub oil_trend: Option<TrendDirection>,
    pub oil_atr_percentile: Option<f64>,
    pub oil_adx: Option<f64>,
    
    // Natural Gas data
    pub natgas_trend: Option<TrendDirection>,
    pub natgas_atr_percentile: Option<f64>,
    pub natgas_adx: Option<f64>,
    
    // === Per-instrument full technicals (H4 - legacy, now also serves as HTF) ===
    pub gold_technicals: Option<InstrumentTechnicals>,
    pub btc_technicals: Option<InstrumentTechnicals>,
    pub eur_usd_technicals: Option<InstrumentTechnicals>,
    pub usd_jpy_technicals: Option<InstrumentTechnicals>,
    pub oil_technicals: Option<InstrumentTechnicals>,
    pub natgas_technicals: Option<InstrumentTechnicals>,
    
    // === Multi-Timeframe Technicals (H1 - mid timeframe) ===
    pub gold_technicals_h1: Option<InstrumentTechnicals>,
    pub btc_technicals_h1: Option<InstrumentTechnicals>,
    pub eur_usd_technicals_h1: Option<InstrumentTechnicals>,
    pub usd_jpy_technicals_h1: Option<InstrumentTechnicals>,
    pub oil_technicals_h1: Option<InstrumentTechnicals>,
    pub natgas_technicals_h1: Option<InstrumentTechnicals>,
    
    // === Multi-Timeframe Technicals (M15 - low timeframe / execution) ===
    pub gold_technicals_m15: Option<InstrumentTechnicals>,
    pub btc_technicals_m15: Option<InstrumentTechnicals>,
    pub eur_usd_technicals_m15: Option<InstrumentTechnicals>,
    pub usd_jpy_technicals_m15: Option<InstrumentTechnicals>,
    pub oil_technicals_m15: Option<InstrumentTechnicals>,
    pub natgas_technicals_m15: Option<InstrumentTechnicals>,
    
    // === Multi-Timeframe Technicals (D1 - daily / trend bias) ===
    pub gold_technicals_d1: Option<InstrumentTechnicals>,
    pub btc_technicals_d1: Option<InstrumentTechnicals>,
    pub eur_usd_technicals_d1: Option<InstrumentTechnicals>,
    pub usd_jpy_technicals_d1: Option<InstrumentTechnicals>,
    pub oil_technicals_d1: Option<InstrumentTechnicals>,
    pub natgas_technicals_d1: Option<InstrumentTechnicals>,
    
    // === Multi-Timeframe ADX values ===
    pub gold_adx_h1: Option<f64>,
    pub gold_adx_m15: Option<f64>,
    pub btc_adx_h1: Option<f64>,
    pub btc_adx_m15: Option<f64>,
    pub eur_usd_adx_h1: Option<f64>,
    pub eur_usd_adx_m15: Option<f64>,
    pub usd_jpy_adx_h1: Option<f64>,
    pub usd_jpy_adx_m15: Option<f64>,
    pub oil_adx_h1: Option<f64>,
    pub oil_adx_m15: Option<f64>,
    pub natgas_adx_h1: Option<f64>,
    pub natgas_adx_m15: Option<f64>,
    // D1 ADX values
    pub gold_adx_d1: Option<f64>,
    pub btc_adx_d1: Option<f64>,
    pub eur_usd_adx_d1: Option<f64>,
    pub usd_jpy_adx_d1: Option<f64>,
    pub oil_adx_d1: Option<f64>,
    pub natgas_adx_d1: Option<f64>,
    
    // News sentiment (from Alpha Vantage)
    /// Overall news sentiment (-1.0 bearish to 1.0 bullish)
    pub news_sentiment_overall: Option<f64>,
    /// Gold-specific news sentiment (-1.0 to 1.0)
    pub news_sentiment_gold: Option<f64>,
    /// Crypto/BTC news sentiment (-1.0 to 1.0)
    pub news_sentiment_crypto: Option<f64>,
    /// Economy/monetary policy news sentiment (-1.0 to 1.0)
    pub news_sentiment_economy: Option<f64>,
    /// Sentiment dispersion (high = conflicting signals)
    pub news_sentiment_dispersion: Option<f64>,
    /// Whether news shows bearish consensus
    pub news_bearish_consensus: bool,
    /// Whether news shows bullish consensus
    pub news_bullish_consensus: bool,
    
    // Reddit sentiment (social media)
    /// Overall crypto sentiment from Reddit (-1.0 to 1.0)
    pub reddit_crypto_sentiment: Option<f64>,
    /// Bitcoin-specific sentiment from Reddit
    pub reddit_btc_sentiment: Option<f64>,
    /// Gold mentions sentiment from Reddit
    pub reddit_gold_sentiment: Option<f64>,
    /// Reddit sentiment dispersion
    pub reddit_dispersion: Option<f64>,
    /// Reddit bullish consensus
    pub reddit_bullish_consensus: bool,
    /// Reddit bearish consensus
    pub reddit_bearish_consensus: bool,
    
    // Alternative.me Fear & Greed (backup/validation for CNN index)
    pub alt_fear_greed_index: Option<f64>,
    
    // Regime Event Detection (from NewsAPI)
    /// Detected regime-shifting event (Fed chair change, crisis, etc.)
    pub regime_event: Option<String>,
    /// Headline that triggered the regime event detection
    pub regime_event_headline: Option<String>,
    /// Uncertainty boost from regime event (0-35)
    pub regime_event_uncertainty_boost: f64,
    /// Risk reduction multiplier from regime event (0.4-1.0)
    pub regime_event_risk_reduction: f64,
    
    // Trade Feedback from AI Brain (position-aware inputs)
    /// Whether there are currently open positions
    pub has_open_positions: bool,
    /// Number of open trades
    pub open_trade_count: i32,
    /// Total unrealized P&L across all positions
    pub total_unrealized_pnl: Option<f64>,
    /// Net direction bias: -1.0 (all short) to +1.0 (all long)
    pub net_position_bias: Option<f64>,
    /// Percentage of positions aligned with current regime
    pub position_regime_alignment: Option<f64>,
    /// Number of positions flagged for attention (ConsiderClose or CloseNow)
    pub positions_at_risk: i32,
    /// Position stress score (0-100, based on attention ratio + concentration)
    pub position_stress_score: Option<f64>,
}

impl Default for MrateInputs {
    fn default() -> Self {
        Self {
            rate_cut_probability: None,
            inflation_expectations: None,
            safe_haven_demand: None,
            btc_sentiment: None,
            fed_uncertainty: None,
            dxy_trend: None,
            sp500_trend: None,
            yield_10y_trend: None,
            vix_level: None,
            dxy_level: None,
            fear_greed_index: None,
            fear_greed_category: None,
            yield_10y: None,
            yield_2y: None,
            real_yield_10y: None,
            stablecoin_dominance: None,
            btc_exchange_netflow: None,
            atr_percentile: None,
            gold_adx: None,
            gold_etf_flow_weekly: None,
            high_impact_event_within_24h: false,
            hours_to_next_high_impact: None,
            next_event_weight: 1.0,
            btc_trend: None,
            btc_atr_percentile: None,
            btc_adx: None,
            eur_usd_trend: None,
            eur_usd_atr_percentile: None,
            eur_usd_adx: None,
            usd_jpy_trend: None,
            usd_jpy_atr_percentile: None,
            usd_jpy_adx: None,
            oil_trend: None,
            oil_atr_percentile: None,
            oil_adx: None,
            natgas_trend: None,
            natgas_atr_percentile: None,
            natgas_adx: None,
            // Per-instrument technicals (H4 / HTF)
            gold_technicals: None,
            btc_technicals: None,
            eur_usd_technicals: None,
            usd_jpy_technicals: None,
            oil_technicals: None,
            natgas_technicals: None,
            // H1 technicals (MTF)
            gold_technicals_h1: None,
            btc_technicals_h1: None,
            eur_usd_technicals_h1: None,
            usd_jpy_technicals_h1: None,
            oil_technicals_h1: None,
            natgas_technicals_h1: None,
            // M15 technicals (LTF)
            gold_technicals_m15: None,
            btc_technicals_m15: None,
            eur_usd_technicals_m15: None,
            usd_jpy_technicals_m15: None,
            oil_technicals_m15: None,
            natgas_technicals_m15: None,
            // D1 technicals (daily trend bias)
            gold_technicals_d1: None,
            btc_technicals_d1: None,
            eur_usd_technicals_d1: None,
            usd_jpy_technicals_d1: None,
            oil_technicals_d1: None,
            natgas_technicals_d1: None,
            // MTF ADX values
            gold_adx_h1: None,
            gold_adx_m15: None,
            btc_adx_h1: None,
            btc_adx_m15: None,
            eur_usd_adx_h1: None,
            eur_usd_adx_m15: None,
            usd_jpy_adx_h1: None,
            usd_jpy_adx_m15: None,
            oil_adx_h1: None,
            oil_adx_m15: None,
            natgas_adx_h1: None,
            natgas_adx_m15: None,
            // D1 ADX values
            gold_adx_d1: None,
            btc_adx_d1: None,
            eur_usd_adx_d1: None,
            usd_jpy_adx_d1: None,
            oil_adx_d1: None,
            natgas_adx_d1: None,
            news_sentiment_overall: None,
            news_sentiment_gold: None,
            news_sentiment_crypto: None,
            news_sentiment_economy: None,
            news_sentiment_dispersion: None,
            news_bearish_consensus: false,
            news_bullish_consensus: false,
            reddit_crypto_sentiment: None,
            reddit_btc_sentiment: None,
            reddit_gold_sentiment: None,
            reddit_dispersion: None,
            reddit_bullish_consensus: false,
            reddit_bearish_consensus: false,
            alt_fear_greed_index: None,
            // Regime event defaults
            regime_event: None,
            regime_event_headline: None,
            regime_event_uncertainty_boost: 0.0,
            regime_event_risk_reduction: 1.0,
            // Trade feedback defaults
            has_open_positions: false,
            open_trade_count: 0,
            total_unrealized_pnl: None,
            net_position_bias: None,
            position_regime_alignment: None,
            positions_at_risk: 0,
            position_stress_score: None,
        }
    }
}

/// Instrument recommendation based on current regime (legacy, kept for backward compat)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FavoredInstrument {
    Gold,
    Bitcoin,
    Both,
    Neither,
}

impl FavoredInstrument {
    pub fn as_str(&self) -> &'static str {
        match self {
            FavoredInstrument::Gold => "GOLD",
            FavoredInstrument::Bitcoin => "BITCOIN",
            FavoredInstrument::Both => "BOTH",
            FavoredInstrument::Neither => "NEITHER",
        }
    }
}

/// Trading direction derived from per-instrument analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradingDirection {
    /// Conditions favor LONG trades
    Long,
    /// Conditions favor SHORT trades
    Short,
    /// No clear directional bias
    #[default]
    Neutral,
}

impl TradingDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradingDirection::Long => "LONG",
            TradingDirection::Short => "SHORT",
            TradingDirection::Neutral => "NEUTRAL",
        }
    }
}

/// Per-instrument technical indicators computed from price data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstrumentTechnicals {
    /// RSI (14-period) — 0-100, >70 overbought, <30 oversold
    pub rsi: f64,
    /// Bollinger %B — 0.0 at lower band, 0.5 at middle, 1.0 at upper band
    /// >1.0 = above upper band, <0.0 = below lower band
    pub bollinger_percent_b: f64,
    /// MACD histogram value — positive = bullish momentum, negative = bearish
    pub macd_histogram: f64,
    /// +DI (positive directional indicator) from ADX — higher = stronger upward pressure
    pub plus_di: f64,
    /// -DI (negative directional indicator) from ADX — higher = stronger downward pressure
    pub minus_di: f64,
    /// Stochastic %K (14-period) — 0-100, >80 overbought, <20 oversold
    pub stochastic_k: f64,
    /// Stochastic %D (3-period SMA of %K)
    pub stochastic_d: f64,
    /// Composite momentum score 0-100
    /// Combines RSI direction, MACD, +DI/-DI, Stochastic
    /// >60 = bullish momentum, <40 = bearish momentum
    pub momentum_score: f64,
    /// Current price (latest close)
    pub current_price: f64,
    /// EMA 50 value
    pub ema_50: f64,
    /// EMA 200 value  
    pub ema_200: f64,
    /// ATR value (14-period)
    pub atr: f64,
}

impl InstrumentTechnicals {
    /// Derive TradingDirection from single timeframe technicals (LEGACY - prefer derive_direction_mtf)
    pub fn derive_direction(&self, macro_score: f64) -> TradingDirection {
        let mut bullish_signals = 0i32;
        let mut bearish_signals = 0i32;
        
        // +DI vs -DI (most reliable directional indicator)
        if self.plus_di > self.minus_di + 5.0 { bullish_signals += 2; }
        else if self.minus_di > self.plus_di + 5.0 { bearish_signals += 2; }
        
        // MACD histogram (momentum direction)
        if self.macd_histogram > 0.0 { bullish_signals += 1; }
        else if self.macd_histogram < 0.0 { bearish_signals += 1; }
        
        // RSI zone (not extreme = momentum confirmation)
        if self.rsi > 50.0 && self.rsi < 70.0 { bullish_signals += 1; }  // Bullish but not overbought
        else if self.rsi < 50.0 && self.rsi > 30.0 { bearish_signals += 1; }  // Bearish but not oversold
        
        // EMA alignment - REDUCED WEIGHT (was 2, now 1) to be less laggy
        if self.current_price > self.ema_50 && self.ema_50 > self.ema_200 { bullish_signals += 1; }  // Golden cross zone
        else if self.current_price < self.ema_50 && self.ema_50 < self.ema_200 { bearish_signals += 1; }  // Death cross zone
        
        // Bollinger position (extreme = caution, mid = confirmation)
        if self.bollinger_percent_b > 0.5 && self.bollinger_percent_b < 0.9 { bullish_signals += 1; }
        else if self.bollinger_percent_b < 0.5 && self.bollinger_percent_b > 0.1 { bearish_signals += 1; }
        
        // Macro score influence
        if macro_score > 60.0 { bullish_signals += 1; }
        else if macro_score < 40.0 { bearish_signals += 1; }
        
        // Reduced threshold from 3 to 2 for faster response
        let net = bullish_signals - bearish_signals;
        if net >= 2 {
            TradingDirection::Long
        } else if net <= -2 {
            TradingDirection::Short
        } else {
            TradingDirection::Neutral
        }
    }
    
    /// Fast direction check using only price action and momentum (no lagging EMAs)
    /// Returns a score: positive = bullish, negative = bearish
    pub fn quick_direction_score(&self) -> f64 {
        let mut score = 0.0;
        
        // +DI vs -DI (primary direction indicator, fast)
        let di_diff = self.plus_di - self.minus_di;
        score += di_diff * 0.3;  // Scale: +30 DI diff = +9 points
        
        // MACD histogram (momentum)
        if self.macd_histogram > 0.0 { score += 2.0; }
        else if self.macd_histogram < 0.0 { score -= 2.0; }
        
        // RSI direction (not level, just above/below 50)
        if self.rsi > 55.0 { score += 1.5; }
        else if self.rsi < 45.0 { score -= 1.5; }
        
        // Price vs EMA 50 only (faster than EMA 200)
        if self.current_price > self.ema_50 { score += 1.0; }
        else if self.current_price < self.ema_50 { score -= 1.0; }
        
        // Momentum score (composite)
        score += (self.momentum_score - 50.0) * 0.1;  // ±5 points max
        
        score
    }
}

/// Simple D1 Direction - Institutional approach
/// Price above 200 EMA = LONG, Price below 200 EMA = SHORT
/// This is what billion-dollar CTAs use.
pub fn derive_direction_d1_ema(
    d1_technicals: Option<&InstrumentTechnicals>,
) -> TradingDirection {
    let Some(d1) = d1_technicals else {
        // No D1 data = allow both directions (fail open)
        return TradingDirection::Neutral;
    };
    
    // Simple: price vs 200 EMA
    if d1.ema_200 <= 0.0 || d1.current_price <= 0.0 {
        return TradingDirection::Neutral;
    }
    
    if d1.current_price > d1.ema_200 {
        TradingDirection::Long
    } else {
        TradingDirection::Short
    }
}

/// Legacy MTF direction (kept for compatibility, now uses D1 EMA internally)
pub fn derive_direction_mtf(
    d1_technicals: Option<&InstrumentTechnicals>,
    _h1_technicals: Option<&InstrumentTechnicals>,
    _m15_technicals: Option<&InstrumentTechnicals>,
    _macro_score: f64,
) -> TradingDirection {
    // Now just delegates to the simple D1 EMA approach
    derive_direction_d1_ema(d1_technicals)
}

/// Old complex MTF calculation - DEPRECATED
/// Kept here in case we need to reference it, but not used
#[allow(dead_code)]
fn derive_direction_mtf_complex(
    d1_technicals: Option<&InstrumentTechnicals>,
    h1_technicals: Option<&InstrumentTechnicals>,
    m15_technicals: Option<&InstrumentTechnicals>,
    macro_score: f64,
) -> TradingDirection {
    // Weights for each timeframe
    const D1_WEIGHT: f64 = 0.50;   // Daily trend bias (what you see on the chart)
    const H1_WEIGHT: f64 = 0.30;   // Hourly confirmation
    const M15_WEIGHT: f64 = 0.20;  // 15min execution timing
    
    let mut total_score = 0.0;
    let mut total_weight = 0.0;
    
    // D1 (Daily) - PRIMARY trend bias
    // This is what you see when you look at the daily chart
    if let Some(d1) = d1_technicals {
        let d1_score = d1.quick_direction_score();
        total_score += d1_score * D1_WEIGHT;
        total_weight += D1_WEIGHT;
        
        // STRONG D1 signal can override lower timeframes
        // If D1 has +DI > -DI by 15+ points with momentum > 60, it's clearly bullish
        if d1.plus_di > d1.minus_di + 15.0 && d1.momentum_score > 60.0 {
            total_score += 5.0 * D1_WEIGHT;  // Extra bullish boost
        } else if d1.minus_di > d1.plus_di + 15.0 && d1.momentum_score < 40.0 {
            total_score -= 5.0 * D1_WEIGHT;  // Extra bearish boost
        }
    }
    
    // H1 (Hourly) - Confirmation
    if let Some(h1) = h1_technicals {
        let h1_score = h1.quick_direction_score();
        total_score += h1_score * H1_WEIGHT;
        total_weight += H1_WEIGHT;
    }
    
    // M15 (15-minute) - Execution timing
    if let Some(m15) = m15_technicals {
        let m15_score = m15.quick_direction_score();
        total_score += m15_score * M15_WEIGHT;
        total_weight += M15_WEIGHT;
    }
    
    // Add macro score influence
    if macro_score > 60.0 {
        total_score += 2.0;
    } else if macro_score < 40.0 {
        total_score -= 2.0;
    }
    
    // If we have no MTF data at all, use macro score alone for direction
    // This prevents getting stuck on Neutral when OANDA data fails
    if total_weight < 0.1 {
        tracing::warn!("No MTF technicals available - using macro score only for direction");
        if macro_score >= 60.0 {
            return TradingDirection::Long;
        } else if macro_score <= 40.0 {
            return TradingDirection::Short;
        } else {
            return TradingDirection::Neutral;
        }
    }
    
    // Normalize score by weight
    let normalized_score = total_score / total_weight.max(1.0);
    
    // Thresholds for direction call
    // Lower threshold (3.0) makes it more responsive to changes
    if normalized_score >= 3.0 {
        TradingDirection::Long
    } else if normalized_score <= -3.0 {
        TradingDirection::Short
    } else {
        TradingDirection::Neutral
    }
}

impl InstrumentTechnicals {
    /// Best strategy categories for current conditions
    pub fn best_strategy_types(&self, price_regime: PriceRegime) -> Vec<StrategyCategory> {
        let mut strategies = Vec::new();
        
        match price_regime {
            PriceRegime::Trending => {
                strategies.push(StrategyCategory::Trend);
                // If trending strongly with high ADX, breakouts also work
                if self.momentum_score > 65.0 || self.momentum_score < 35.0 {
                    strategies.push(StrategyCategory::Breakout);
                }
            },
            PriceRegime::Ranging => {
                strategies.push(StrategyCategory::MeanReversion);
                // If Bollinger is at extremes, strong MR signal
                if self.bollinger_percent_b > 0.95 || self.bollinger_percent_b < 0.05 {
                    strategies.push(StrategyCategory::LiquiditySweep);
                }
            },
            PriceRegime::Transitional => {
                // Both can work
                strategies.push(StrategyCategory::Trend);
                strategies.push(StrategyCategory::MeanReversion);
            },
        }
        
        strategies
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MULTI-TIMEFRAME (MTF) CONFLUENCE SCORING
// ═══════════════════════════════════════════════════════════════════════════════

/// Timeframe label for multi-timeframe analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Timeframe {
    /// Higher timeframe (H4) - trend anchor
    H4,
    /// Mid timeframe (H1) - confirmation
    H1,
    /// Lower timeframe (M15) - execution timing
    M15,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::H4 => "H4",
            Timeframe::H1 => "H1",
            Timeframe::M15 => "M15",
        }
    }
    
    /// Weight for confluence calculation (HTF=50%, MTF=30%, LTF=20%)
    pub fn confluence_weight(&self) -> f64 {
        match self {
            Timeframe::H4 => 0.50,
            Timeframe::H1 => 0.30,
            Timeframe::M15 => 0.20,
        }
    }
}

/// Technicals for a specific timeframe
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeframeTechnicals {
    /// Which timeframe this data is from
    pub timeframe: String,
    /// Full technical indicators
    pub technicals: InstrumentTechnicals,
    /// ADX value for this timeframe
    pub adx: f64,
    /// Derived trading direction for this timeframe
    pub direction: TradingDirection,
    /// Price regime (Trending/Ranging/Transitional)
    pub price_regime: PriceRegime,
    /// Trend direction (StrongUp/Up/Flat/Down/StrongDown)
    pub trend_direction: TrendDirection,
}

impl TimeframeTechnicals {
    /// Create from raw technicals and ADX
    pub fn from_technicals(timeframe: &str, technicals: InstrumentTechnicals, adx: f64, macro_score: f64) -> Self {
        let direction = technicals.derive_direction(macro_score);
        let price_regime = PriceRegime::from_adx(adx);
        
        // Derive trend direction from +DI/-DI and momentum
        let trend_direction = if technicals.plus_di > technicals.minus_di + 10.0 && technicals.momentum_score > 60.0 {
            TrendDirection::StrongUp
        } else if technicals.plus_di > technicals.minus_di + 3.0 {
            TrendDirection::Up
        } else if technicals.minus_di > technicals.plus_di + 10.0 && technicals.momentum_score < 40.0 {
            TrendDirection::StrongDown
        } else if technicals.minus_di > technicals.plus_di + 3.0 {
            TrendDirection::Down
        } else {
            TrendDirection::Flat
        };
        
        Self {
            timeframe: timeframe.to_string(),
            technicals,
            adx,
            direction,
            price_regime,
            trend_direction,
        }
    }
}

/// Alignment status across timeframes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimeframeAlignment {
    /// All 3 timeframes agree on direction
    Aligned,
    /// 2 of 3 timeframes agree (usually HTF+MTF or MTF+LTF)
    PartiallyAligned,
    /// Timeframes disagree - high risk of whipsaw
    #[default]
    Divergent,
}

impl TimeframeAlignment {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeframeAlignment::Aligned => "ALIGNED",
            TimeframeAlignment::PartiallyAligned => "PARTIALLY_ALIGNED",
            TimeframeAlignment::Divergent => "DIVERGENT",
        }
    }
    
    /// Calculate alignment from 3 directions
    pub fn from_directions(htf: TradingDirection, mtf: TradingDirection, ltf: TradingDirection) -> Self {
        let htf_bull = matches!(htf, TradingDirection::Long);
        let htf_bear = matches!(htf, TradingDirection::Short);
        let mtf_bull = matches!(mtf, TradingDirection::Long);
        let mtf_bear = matches!(mtf, TradingDirection::Short);
        let ltf_bull = matches!(ltf, TradingDirection::Long);
        let ltf_bear = matches!(ltf, TradingDirection::Short);
        
        // All agree bullish or bearish
        if (htf_bull && mtf_bull && ltf_bull) || (htf_bear && mtf_bear && ltf_bear) {
            return TimeframeAlignment::Aligned;
        }
        
        // 2 of 3 agree (including neutral)
        let bull_count = [htf_bull, mtf_bull, ltf_bull].iter().filter(|&&x| x).count();
        let bear_count = [htf_bear, mtf_bear, ltf_bear].iter().filter(|&&x| x).count();
        
        if bull_count >= 2 || bear_count >= 2 {
            return TimeframeAlignment::PartiallyAligned;
        }
        
        // HTF + MTF agree (stronger signal even if LTF diverges)
        if (htf_bull && mtf_bull) || (htf_bear && mtf_bear) {
            return TimeframeAlignment::PartiallyAligned;
        }
        
        TimeframeAlignment::Divergent
    }
}

/// Multi-timeframe data for an instrument
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultiTimeframeData {
    /// Higher timeframe (H4) - trend anchor, don't fight this
    pub htf: TimeframeTechnicals,
    /// Mid timeframe (H1) - confirmation layer
    pub mtf: TimeframeTechnicals,
    /// Lower timeframe (M15) - execution timing
    pub ltf: TimeframeTechnicals,
    /// Weighted confluence score (0-100)
    pub confluence_score: f64,
    /// Alignment status across timeframes
    pub alignment: TimeframeAlignment,
    /// Recommended direction based on HTF bias + confluence
    pub recommended_direction: TradingDirection,
    /// Confidence level based on alignment (0.0-1.0)
    pub confidence: f64,
}

impl MultiTimeframeData {
    /// Calculate confluence from 3 timeframes
    pub fn calculate(htf: TimeframeTechnicals, mtf: TimeframeTechnicals, ltf: TimeframeTechnicals) -> Self {
        let alignment = TimeframeAlignment::from_directions(htf.direction, mtf.direction, ltf.direction);
        
        // Direction scoring: Long = +1, Neutral = 0, Short = -1
        let dir_score = |d: TradingDirection| -> f64 {
            match d {
                TradingDirection::Long => 1.0,
                TradingDirection::Neutral => 0.0,
                TradingDirection::Short => -1.0,
            }
        };
        
        // Weighted direction score: HTF 50%, MTF 30%, LTF 20%
        let weighted_dir = dir_score(htf.direction) * 0.50
            + dir_score(mtf.direction) * 0.30
            + dir_score(ltf.direction) * 0.20;
        
        // Convert weighted direction (-1 to +1) to confluence score (0-100)
        // +1.0 = 100 (strong long confluence)
        // -1.0 = 0 (strong short confluence)
        // 0.0 = 50 (neutral)
        let confluence_score = (weighted_dir + 1.0) * 50.0;
        
        // Recommended direction follows HTF unless strong LTF divergence
        let recommended_direction = if alignment == TimeframeAlignment::Aligned {
            htf.direction
        } else if alignment == TimeframeAlignment::PartiallyAligned {
            // Follow HTF if it has a bias, otherwise follow MTF
            if htf.direction != TradingDirection::Neutral {
                htf.direction
            } else {
                mtf.direction
            }
        } else {
            // Divergent - stay neutral or follow HTF cautiously
            if htf.direction != TradingDirection::Neutral && htf.adx > 30.0 {
                htf.direction // Strong HTF trend overrides LTF noise
            } else {
                TradingDirection::Neutral
            }
        };
        
        // Confidence based on alignment
        let confidence = match alignment {
            TimeframeAlignment::Aligned => 1.0,
            TimeframeAlignment::PartiallyAligned => 0.7,
            TimeframeAlignment::Divergent => 0.3,
        };
        
        Self {
            htf,
            mtf,
            ltf,
            confluence_score,
            alignment,
            recommended_direction,
            confidence,
        }
    }
}

/// Individual instrument score with reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentScore {
    /// Symbol (e.g., "XAU_USD")
    pub symbol: String,
    /// Score 0-100 (higher = more favorable for LONGS based on macro)
    pub score: f64,
    /// Trading recommendation (legacy, directional-unaware)
    pub recommendation: InstrumentRecommendation,
    /// Key factors influencing the score
    pub factors: Vec<String>,
    /// Current price trend direction (from ADX/price action)
    #[serde(default)]
    pub trend_direction: TrendDirection,
    /// Trend strength 0-100 (based on ADX value)
    #[serde(default)]
    pub trend_strength: f64,
    /// Whether instrument is trending or ranging
    #[serde(default)]
    pub price_regime: PriceRegime,
    /// Per-instrument technical indicators
    #[serde(default)]
    pub technicals: InstrumentTechnicals,
    /// Directional trading bias derived from score + technicals
    #[serde(default)]
    pub trading_direction: TradingDirection,
    /// Best strategy categories for current conditions
    #[serde(default)]
    pub best_strategies: Vec<StrategyCategory>,
    /// Multi-timeframe confluence data (H4/H1/M15)
    #[serde(default)]
    pub mtf_data: Option<MultiTimeframeData>,
    
    // ═══════════════════════════════════════════════════════════════════
    // ATR-BASED STOP RECOMMENDATIONS (from D1 timeframe)
    // These provide "room to breathe" stops based on daily volatility
    // ═══════════════════════════════════════════════════════════════════
    
    /// D1 (daily) ATR value - represents true instrument volatility
    #[serde(default)]
    pub d1_atr: f64,
    
    /// Recommended stop loss distance in price units (2x D1 ATR default)
    /// This gives the trade room to breathe through normal daily swings
    #[serde(default)]
    pub recommended_stop_distance: f64,
    
    /// Recommended take profit distance in price units (3x D1 ATR for 1.5:1 R:R)
    #[serde(default)]
    pub recommended_tp_distance: f64,
    
    /// ATR-based stop multiplier used (can be adjusted by price regime)
    #[serde(default = "default_stop_mult")]
    pub stop_atr_multiplier: f64,
    
    // ═══════════════════════════════════════════════════════════════════
    // D1 DIRECTION REFERENCE VALUES
    // Shows what the direction decision is based on
    // ═══════════════════════════════════════════════════════════════════
    
    /// D1 200 EMA value - the key level for direction
    /// Price above this = LONG, below = SHORT
    #[serde(default)]
    pub ema_200: f64,
    
    /// Current D1 close price
    #[serde(default)]
    pub current_price: f64,
}

fn default_stop_mult() -> f64 { 2.0 }

/// Price-based regime for an instrument (independent of macro regime)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PriceRegime {
    /// Strong trend with ADX > 25 - favor trend strategies
    Trending,
    /// No clear trend, ADX < 20 - favor mean reversion
    #[default]
    Ranging,
    /// Transitional, ADX 20-25 - either strategy may work
    Transitional,
}

impl PriceRegime {
    pub fn as_str(&self) -> &'static str {
        match self {
            PriceRegime::Trending => "TRENDING",
            PriceRegime::Ranging => "RANGING",
            PriceRegime::Transitional => "TRANSITIONAL",
        }
    }
    
    pub fn from_adx(adx: f64) -> Self {
        if adx >= 25.0 {
            PriceRegime::Trending
        } else if adx <= 20.0 {
            PriceRegime::Ranging
        } else {
            PriceRegime::Transitional
        }
    }
    
    /// Whether trend strategies should be allowed
    pub fn allows_trend_strategies(&self) -> bool {
        matches!(self, PriceRegime::Trending | PriceRegime::Transitional)
    }
    
    /// Whether mean reversion strategies should be allowed  
    pub fn allows_mean_reversion(&self) -> bool {
        matches!(self, PriceRegime::Ranging | PriceRegime::Transitional)
    }
}

/// Trading recommendation for an instrument
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstrumentRecommendation {
    /// Strong conditions - actively seek trades
    StrongBuy,
    /// Favorable conditions - trade normally
    Buy,
    /// Neutral - be selective
    Neutral,
    /// Unfavorable - reduce exposure
    Avoid,
    /// Poor conditions - stay out
    StrongAvoid,
}

impl InstrumentRecommendation {
    pub fn from_score(score: f64) -> Self {
        if score >= 75.0 {
            InstrumentRecommendation::StrongBuy
        } else if score >= 60.0 {
            InstrumentRecommendation::Buy
        } else if score >= 40.0 {
            InstrumentRecommendation::Neutral
        } else if score >= 25.0 {
            InstrumentRecommendation::Avoid
        } else {
            InstrumentRecommendation::StrongAvoid
        }
    }
}

/// Scores for all tradeable instruments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentScores {
    /// Gold (XAU/USD) - Safe haven
    pub gold: InstrumentScore,
    /// Bitcoin (BTC/USD) - Risk-on crypto
    pub bitcoin: InstrumentScore,
    /// EUR/USD - Major forex pair
    pub eur_usd: InstrumentScore,
    /// USD/JPY - Carry trade pair
    pub usd_jpy: InstrumentScore,
    /// WTI Crude Oil - Energy commodity
    pub wti_oil: InstrumentScore,
    /// Natural Gas - Volatile energy
    pub natural_gas: InstrumentScore,
}

impl InstrumentScores {
    /// Get the top N instruments by score
    pub fn top_instruments(&self, n: usize) -> Vec<&InstrumentScore> {
        let mut all = vec![
            &self.gold, &self.bitcoin, &self.eur_usd,
            &self.usd_jpy, &self.wti_oil, &self.natural_gas,
        ];
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all.into_iter().take(n).collect()
    }
    
    /// Get score for a specific symbol
    pub fn get_score(&self, symbol: &str) -> Option<&InstrumentScore> {
        match symbol {
            "XAU_USD" | "XAUUSD" => Some(&self.gold),
            "BTC_USD" | "BTCUSD" => Some(&self.bitcoin),
            "EUR_USD" | "EURUSD" => Some(&self.eur_usd),
            "USD_JPY" | "USDJPY" => Some(&self.usd_jpy),
            "WTICO_USD" | "WTICOUSD" => Some(&self.wti_oil),
            "NATGAS_USD" | "NATGASUSD" => Some(&self.natural_gas),
            _ => None,
        }
    }
    
    /// Get legacy FavoredInstrument (for backward compat)
    pub fn to_favored_instrument(&self) -> FavoredInstrument {
        let gold_good = self.gold.score >= 60.0;
        let btc_good = self.bitcoin.score >= 60.0;
        
        match (gold_good, btc_good) {
            (true, true) => FavoredInstrument::Both,
            (true, false) => FavoredInstrument::Gold,
            (false, true) => FavoredInstrument::Bitcoin,
            (false, false) => FavoredInstrument::Neither,
        }
    }
}

use super::thresholds::DynamicThresholds;

/// The main MRATE output consumed by bots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrateOutput {
    pub timestamp: DateTime<Utc>,
    pub regime: Regime,
    pub liquidity_score: f64,
    pub risk_score: f64,
    pub uncertainty_score: f64,
    pub risk_multiplier: f64,
    pub strategy_weights: StrategyWeights,
    pub favored_instrument: FavoredInstrument,
    
    /// Detailed scores for all tradeable instruments
    pub instrument_scores: InstrumentScores,
    
    /// Dynamic thresholds for strategy parameter adjustment
    pub thresholds: DynamicThresholds,
    
    /// Regime confidence (0.0-1.0) — distance from boundary thresholds
    pub regime_confidence: f64,
    
    /// Proposed regime (before hysteresis filtering)
    pub proposed_regime: Regime,
    
    /// True if a regime change is pending (waiting for confirmation)
    pub regime_change_pending: bool,
    
    /// Liquidity momentum (positive = improving, negative = deteriorating)
    pub liquidity_momentum: f64,
    /// Risk momentum (positive = risk increasing, negative = decreasing)
    pub risk_momentum: f64,
    
    /// Data source health status
    pub data_health: DataHealth,
    
    /// True if operating in degraded mode (some feeds down)
    pub degraded_mode: bool,
    
    /// Raw inputs used for calculation (for debugging/display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<MrateInputs>,
}

impl MrateOutput {
    /// Check if a strategy should trade based on its category weight
    /// NOTE: This is the legacy method - prefer should_trade_instrument for direction-aware logic
    pub fn should_trade(&self, category: StrategyCategory) -> bool {
        self.strategy_weights.get(category) >= 0.4
    }
    
    /// Check if a strategy should trade on a specific instrument with direction awareness
    /// 
    /// MRATE is ADVISORY, not a blocker. The bot's own signal is the authority.
    /// MRATE adjusts position size based on how favorable conditions are.
    /// 
    /// Per-instrument data informs the weight (position size multiplier):
    /// - Favorable conditions (aligned direction, good regime) → high weight (70-95%)
    /// - Neutral conditions → medium weight (40-60%)
    /// - Opposing conditions → low weight (15-30%) but STILL ALLOWED
    /// 
    /// The bot's confidence threshold (0.6) is the real gate. MRATE just sizes.
    /// 
    /// Returns (should_trade, adjusted_weight, reason)
    pub fn should_trade_instrument(
        &self, 
        category: StrategyCategory, 
        instrument: &str,
        direction: Option<&str>,  // "LONG", "SHORT", or None
    ) -> (bool, f64, String) {
        let base_weight = self.strategy_weights.get(category);
        let instrument_score = self.instrument_scores.get_score(instrument);
        
        // No instrument data → fall back to global weight (always allow)
        let inst = match instrument_score {
            Some(s) => s,
            None => return (true, base_weight.max(0.4), "No instrument data — using global weight".to_string()),
        };
        
        let adx = inst.trend_strength;
        let price_regime = inst.price_regime;
        let trend_dir = inst.trend_direction;
        let score = inst.score;
        let trading_direction = inst.trading_direction;
        
        // ═══════════════════════════════════════════════════════════════
        // RULE 1: Per-instrument best_strategies match = OPTIMAL
        // The instrument's technicals recommend this strategy category.
        // Give it high weight.
        // ═══════════════════════════════════════════════════════════════
        if inst.best_strategies.contains(&category) {
            let mut inst_weight = match price_regime {
                PriceRegime::Trending => 0.70 + ((adx - 25.0).max(0.0) / 50.0 * 0.20), // 70-90%
                PriceRegime::Ranging => 0.65,
                PriceRegime::Transitional => 0.55,
            }.min(0.95);
            
            // Direction check: if opposing, REDUCE weight but don't block
            // The bot generated the signal - it may see something MRATE doesn't
            let direction_opposes = match (direction, trading_direction) {
                (Some("LONG"), TradingDirection::Short) => true,
                (Some("SHORT"), TradingDirection::Long) => true,
                _ => false,
            };
            
            if direction_opposes {
                let dir_str = match trading_direction {
                    TradingDirection::Long => "LONG",
                    TradingDirection::Short => "SHORT",
                    TradingDirection::Neutral => "NEUTRAL",
                };
                // Reduce to 25% of optimal weight, but ALLOW the trade
                let reduced_weight = (inst_weight * 0.25).max(0.15);
                return (
                    true,  // ALLOW - bot's signal is authority
                    reduced_weight,
                    format!("⚠️ {} counter-trend: You want {} but H4 technicals say {} — reduced to {:.0}%",
                        instrument, direction.unwrap_or("?"), dir_str, reduced_weight * 100.0)
                );
            }
            
            return (
                true,
                inst_weight,
                format!("✅ {} {} regime recommends {:?} — weight {:.0}%",
                    instrument, price_regime.as_str(), category, inst_weight * 100.0)
            );
        }
        
        // ═══════════════════════════════════════════════════════════════
        // RULE 2: Per-instrument price_regime permits this strategy type
        // Even if not in best_strategies, if the regime allows it, let it through
        // ═══════════════════════════════════════════════════════════════
        let regime_allows = match category {
            StrategyCategory::Trend | StrategyCategory::Breakout => price_regime.allows_trend_strategies(),
            StrategyCategory::MeanReversion | StrategyCategory::LiquiditySweep => price_regime.allows_mean_reversion(),
        };
        
        if regime_allows {
            // Check direction alignment for trend strategies
            if matches!(category, StrategyCategory::Trend | StrategyCategory::Breakout) {
                let direction_ok = match (direction, trend_dir) {
                    (Some("LONG"), TrendDirection::Up | TrendDirection::StrongUp) => true,
                    (Some("SHORT"), TrendDirection::Down | TrendDirection::StrongDown) => true,
                    (Some("LONG"), TrendDirection::Flat) => true,  // Flat is ok, not opposing
                    (Some("SHORT"), TrendDirection::Flat) => true,
                    (None, _) => true,
                    _ => false, // Direction opposes trend
                };
                
                if direction_ok {
                    let inst_weight = match price_regime {
                        PriceRegime::Trending => 0.60 + ((adx - 25.0).max(0.0) / 50.0 * 0.20),
                        PriceRegime::Transitional => 0.50,
                        PriceRegime::Ranging => 0.40,
                    }.min(0.90);
                    
                    return (
                        true,
                        inst_weight,
                        format!("{} {} permits {:?}, direction OK — weight {:.0}%",
                            instrument, price_regime.as_str(), category, inst_weight * 100.0)
                    );
                }
            } else {
                // Mean reversion / liquidity sweep — no direction check needed
                let inst_weight = if price_regime == PriceRegime::Ranging { 0.65 } else { 0.50 };
                return (
                    true,
                    inst_weight,
                    format!("{} {} permits {:?} — weight {:.0}%",
                        instrument, price_regime.as_str(), category, inst_weight * 100.0)
                );
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // RULE 2b: Suboptimal regime — allow with reduced weight
        // A ranging instrument doesn't mean "never trend-trade". It means
        // trends are weaker, so use smaller positions. The bot's own signal
        // confidence is still the primary filter — if a bot generated a
        // valid signal, let it through with reduced lot sizing.
        // ═══════════════════════════════════════════════════════════════
        if !regime_allows {
            let suboptimal_weight = match (category, price_regime) {
                // Trend/Breakout on a ranging instrument — weak trends exist, reduce exposure
                (StrategyCategory::Trend, PriceRegime::Ranging) => 0.35,
                (StrategyCategory::Breakout, PriceRegime::Ranging) => 0.30,
                // MeanReversion on a trending instrument — counter-trend, risky
                (StrategyCategory::MeanReversion, PriceRegime::Trending) => 0.30,
                (StrategyCategory::LiquiditySweep, PriceRegime::Trending) => 0.25,
                // Anything else that fell through
                _ => 0.30,
            };
            
            // Still check direction isn't actively opposing
            let direction_opposes = match (direction, trend_dir) {
                (Some("LONG"), TrendDirection::Down | TrendDirection::StrongDown) => true,
                (Some("SHORT"), TrendDirection::Up | TrendDirection::StrongUp) => true,
                _ => false,
            };
            
            if !direction_opposes {
                return (
                    true,
                    suboptimal_weight,
                    format!("{} {} is suboptimal for {:?} — reduced weight {:.0}% (bot signal still valid)",
                        instrument, price_regime.as_str(), category, suboptimal_weight * 100.0)
                );
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // RULE 3: Trading direction consensus override
        // Multi-indicator consensus (RSI, MACD, +DI/-DI, EMAs, Bollinger)
        // agrees with trade direction. Allow even if price_regime doesn't 
        // perfectly match — covers timeframe mismatches.
        // ═══════════════════════════════════════════════════════════════
        if matches!(category, StrategyCategory::Trend | StrategyCategory::Breakout) {
            let direction_match = match (direction, trading_direction) {
                (Some("LONG"), TradingDirection::Long) => true,
                (Some("SHORT"), TradingDirection::Short) => true,
                _ => false,
            };
            
            if direction_match {
                let inst_weight = 0.50; // Conservative — no regime confirmation
                return (
                    true,
                    inst_weight,
                    format!("{} technicals confirm {} — direction override, weight {:.0}%",
                        instrument,
                        match trading_direction { TradingDirection::Long => "LONG", TradingDirection::Short => "SHORT", _ => "?" },
                        inst_weight * 100.0)
                );
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // RULE 4: Bidirectional score interpretation
        // Low score = SHORT opportunity, high score = LONG opportunity
        // Opposing direction = reduce weight but ALLOW
        // ═══════════════════════════════════════════════════════════════
        if let Some(dir) = direction {
            // Calculate how much the score opposes the direction
            let opposition_strength = match dir {
                "LONG" if score < 50.0 => (50.0 - score) / 50.0,
                "SHORT" if score > 50.0 => (score - 50.0) / 50.0,
                _ => 0.0,
            };
            
            // Strong opposition = heavily reduced weight but ALLOW
            if opposition_strength > 0.4 {
                let reduced_weight = (base_weight * (1.0 - opposition_strength * 0.6)).max(0.15);
                return (
                    true,  // ALLOW - bot's signal is authority
                    reduced_weight,
                    format!("⚠️ {} score {:.0} opposes {} — reduced to {:.0}% (bot signal has priority)",
                        instrument, score, dir, reduced_weight * 100.0)
                );
            }
            
            // Low score + SHORT = favorable opportunity
            if dir == "SHORT" && score < 40.0 {
                let inst_weight = 0.55 + (40.0 - score) / 40.0 * 0.15; // 55-70%
                return (
                    true,
                    inst_weight,
                    format!("✅ {} low score {:.0} favors SHORT — weight {:.0}%", 
                        instrument, score, inst_weight * 100.0)
                );
            }
            
            // High score + LONG = favorable opportunity
            if dir == "LONG" && score > 60.0 {
                let inst_weight = 0.55 + (score - 60.0) / 40.0 * 0.15; // 55-70%
                return (
                    true,
                    inst_weight,
                    format!("✅ {} high score {:.0} favors LONG — weight {:.0}%", 
                        instrument, score, inst_weight * 100.0)
                );
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // FALLBACK: No specific match — use neutral weight, ALLOW trade
        // The bot generated a signal. Trust it. Just use conservative sizing.
        // ═══════════════════════════════════════════════════════════════
        let neutral_weight = base_weight.max(0.30); // At least 30%
        let dir_label = match trading_direction {
            TradingDirection::Long => "bullish",
            TradingDirection::Short => "bearish",
            TradingDirection::Neutral => "neutral",
        };
        (
            true,  // ALWAYS ALLOW - bot's signal is the authority
            neutral_weight,
            format!("ℹ️ {} {} technicals, {} regime — neutral weight {:.0}%",
                instrument, dir_label, price_regime.as_str(), neutral_weight * 100.0)
        )
    }
    
    /// Get adjusted lot size for a strategy
    pub fn adjust_lot_size(&self, base_lot: f64, category: StrategyCategory) -> f64 {
        let weight = self.strategy_weights.get(category);
        base_lot * self.risk_multiplier * weight
    }
    
    /// Get adjusted confidence for a signal
    pub fn adjust_confidence(&self, base_confidence: f64, category: StrategyCategory) -> f64 {
        let weight = self.strategy_weights.get(category);
        (base_confidence * weight).min(1.0)
    }
}

/// Database model for MRATE snapshots
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MrateSnapshot {
    pub id: uuid::Uuid,
    pub timestamp: DateTime<Utc>,
    pub regime: String,
    pub liquidity_score: f32,
    pub risk_score: f32,
    pub uncertainty_score: f32,
    pub risk_multiplier: f32,
    pub strategy_weights: sqlx::types::Json<StrategyWeights>,
    pub raw_inputs: Option<sqlx::types::Json<MrateInputs>>,
    pub created_at: DateTime<Utc>,
}
