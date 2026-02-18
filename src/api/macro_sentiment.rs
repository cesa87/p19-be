use actix_web::{web, HttpResponse};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::macro_sentiment::{PolymarketClient, MacroSentiment, Market, MacroRegimeState, MacroRegime, calculate_regime_from_sentiment};

/// Cached macro sentiment with expiry
struct SentimentCache {
    sentiment: Option<MacroSentiment>,
    markets: Vec<Market>,
    last_fetch: Option<std::time::Instant>,
}

impl Default for SentimentCache {
    fn default() -> Self {
        Self {
            sentiment: None,
            markets: Vec::new(),
            last_fetch: None,
        }
    }
}

lazy_static::lazy_static! {
    static ref CACHE: Arc<RwLock<SentimentCache>> = Arc::new(RwLock::new(SentimentCache::default()));
}

const CACHE_DURATION_SECS: u64 = 300; // 5 minutes

/// GET /api/macro/sentiment
/// Returns aggregated macro sentiment from Polymarket data
async fn get_sentiment() -> HttpResponse {
    let cache = CACHE.read().await;
    
    // Check if cache is valid
    if let (Some(sentiment), Some(last_fetch)) = (&cache.sentiment, cache.last_fetch) {
        if last_fetch.elapsed().as_secs() < CACHE_DURATION_SECS {
            return HttpResponse::Ok().json(sentiment);
        }
    }
    drop(cache);

    // Fetch fresh data
    info!("Fetching fresh macro sentiment from Polymarket");
    let client = PolymarketClient::new();
    let sentiment = client.calculate_macro_sentiment().await;

    // Update cache
    let mut cache = CACHE.write().await;
    cache.sentiment = Some(sentiment.clone());
    cache.last_fetch = Some(std::time::Instant::now());

    HttpResponse::Ok().json(sentiment)
}

/// GET /api/macro/markets
/// Returns raw Polymarket markets being tracked
async fn get_markets() -> HttpResponse {
    let cache = CACHE.read().await;
    
    // Check if we have cached markets
    if let Some(last_fetch) = cache.last_fetch {
        if last_fetch.elapsed().as_secs() < CACHE_DURATION_SECS && !cache.markets.is_empty() {
            return HttpResponse::Ok().json(&cache.markets);
        }
    }
    drop(cache);

    // Fetch fresh markets
    info!("Fetching Polymarket markets");
    let client = PolymarketClient::new();
    
    let mut all_markets = Vec::new();
    
    if let Ok(fed_markets) = client.get_fed_markets().await {
        all_markets.extend(fed_markets);
    }
    
    if let Ok(btc_markets) = client.get_btc_markets().await {
        all_markets.extend(btc_markets);
    }
    
    if let Ok(econ_markets) = client.get_economic_markets().await {
        all_markets.extend(econ_markets);
    }

    // Sort by volume
    all_markets.sort_by(|a, b| b.volume.partial_cmp(&a.volume).unwrap_or(std::cmp::Ordering::Equal));
    
    // Limit to top 20
    all_markets.truncate(20);

    // Update cache
    let mut cache = CACHE.write().await;
    cache.markets = all_markets.clone();

    HttpResponse::Ok().json(all_markets)
}

/// GET /api/macro/fed
/// Returns Fed-specific markets and sentiment
async fn get_fed_sentiment() -> HttpResponse {
    let client = PolymarketClient::new();
    
    match client.get_fed_markets().await {
        Ok(markets) => {
            let mut sentiment = MacroSentiment::default();
            
            // Analyze just Fed markets
            let mut rate_cut_prob = 0.0;
            let mut rate_hike_prob = 0.0;
            let mut count = 0;

            for market in &markets {
                let q = market.question.to_lowercase();
                if q.contains("cut") || q.contains("decrease") {
                    if let Some(p) = market.outcome_prices.first() {
                        rate_cut_prob += p;
                        count += 1;
                    }
                }
                if q.contains("hike") || q.contains("increase") {
                    if let Some(p) = market.outcome_prices.first() {
                        rate_hike_prob += p;
                        count += 1;
                    }
                }
            }

            if count > 0 {
                sentiment.fed_rate_cut_probability = rate_cut_prob / count as f64;
                sentiment.fed_rate_hike_probability = rate_hike_prob / count as f64;
            }
            
            sentiment.markets_analyzed = markets.len();

            HttpResponse::Ok().json(serde_json::json!({
                "sentiment": sentiment,
                "markets": markets
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        }))
    }
}

/// GET /api/macro/regime
/// Returns current macro regime state with score, regime, multiplier, and breakout mode
/// This is the key endpoint for the Macro-Aligned Momentum strategy
async fn get_regime() -> HttpResponse {
    let cache = CACHE.read().await;
    
    // Get sentiment (cached or fresh)
    let sentiment = if let (Some(sentiment), Some(last_fetch)) = (&cache.sentiment, cache.last_fetch) {
        if last_fetch.elapsed().as_secs() < CACHE_DURATION_SECS {
            sentiment.clone()
        } else {
            drop(cache);
            let client = PolymarketClient::new();
            let sentiment = client.calculate_macro_sentiment().await;
            
            // Update cache
            let mut cache = CACHE.write().await;
            cache.sentiment = Some(sentiment.clone());
            cache.last_fetch = Some(std::time::Instant::now());
            sentiment
        }
    } else {
        drop(cache);
        let client = PolymarketClient::new();
        let sentiment = client.calculate_macro_sentiment().await;
        
        // Update cache
        let mut cache = CACHE.write().await;
        cache.sentiment = Some(sentiment.clone());
        cache.last_fetch = Some(std::time::Instant::now());
        sentiment
    };
    
    // Calculate regime from current sentiment
    // Note: In production, this would use MacroHistoryService with DB for proper delta calculation
    // For now, we compute a simplified regime based on current values
    let regime_state = calculate_regime_from_current(&sentiment);
    
    HttpResponse::Ok().json(regime_state)
}

/// Calculate regime from current sentiment (simplified without DB history)
fn calculate_regime_from_current(sentiment: &MacroSentiment) -> MacroRegimeState {
    // Use the confidence_modifier as a proxy for macro direction
    // modifier > 1.0 = bullish, < 1.0 = bearish
    let modifier = sentiment.confidence_modifier;
    
    // Convert modifier to score-like value (-100 to +100)
    // modifier of 0.5 = -50, 1.0 = 0, 1.5 = +50
    let score = (modifier - 1.0) * 100.0;
    
    // Determine regime based on Fed probabilities and sentiment
    let regime = if sentiment.fed_rate_cut_probability > 0.6 || score > 15.0 {
        MacroRegime::RiskOn
    } else if sentiment.fed_rate_hike_probability > 0.4 || score < -15.0 {
        MacroRegime::RiskOff
    } else {
        MacroRegime::Neutral
    };
    
    // Position multiplier based on conviction
    let position_multiplier = if score.abs() > 40.0 {
        2.0
    } else if score.abs() > 25.0 {
        1.5
    } else {
        1.0
    };
    
    MacroRegimeState {
        score,
        regime,
        position_multiplier,
        breakout_mode: false, // Would need DB history to track regime flips
        days_since_flip: None,
        rate_cut_prob: sentiment.fed_rate_cut_probability,
        inflation_prob: sentiment.inflation_expectation,
        recession_prob: sentiment.safe_haven_demand,
        btc_sentiment: sentiment.btc_sentiment_score,
        delta_rate_cut: 0.0, // Would need DB history
        delta_inflation: 0.0,
        delta_recession: 0.0,
        delta_btc: 0.0,
        last_updated: chrono::Utc::now(),
        markets_analyzed: sentiment.markets_analyzed as i32,
    }
}

/// GET /api/macro/btc
/// Returns Bitcoin-specific markets and sentiment
async fn get_btc_sentiment() -> HttpResponse {
    let client = PolymarketClient::new();
    
    match client.get_btc_markets().await {
        Ok(markets) => {
            let mut bullish_signals = Vec::new();
            let mut bearish_signals = Vec::new();

            for market in &markets {
                let q = market.question.to_lowercase();
                if q.contains("above") || q.contains("reach") || q.contains("hit") {
                    if let Some(p) = market.outcome_prices.first() {
                        bullish_signals.push(*p);
                    }
                } else if q.contains("below") || q.contains("drop") {
                    if let Some(p) = market.outcome_prices.first() {
                        bearish_signals.push(*p);
                    }
                }
            }

            let avg_bullish = if bullish_signals.is_empty() { 0.5 } else {
                bullish_signals.iter().sum::<f64>() / bullish_signals.len() as f64
            };
            let avg_bearish = if bearish_signals.is_empty() { 0.5 } else {
                bearish_signals.iter().sum::<f64>() / bearish_signals.len() as f64
            };
            
            let sentiment_score = (avg_bullish - avg_bearish).clamp(-1.0, 1.0);

            HttpResponse::Ok().json(serde_json::json!({
                "btc_bullish_probability": avg_bullish,
                "btc_bearish_probability": avg_bearish,
                "btc_sentiment_score": sentiment_score,
                "markets_analyzed": markets.len(),
                "markets": markets
            }))
        }
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e
        }))
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/macro")
            .route("/sentiment", web::get().to(get_sentiment))
            .route("/markets", web::get().to(get_markets))
            .route("/fed", web::get().to(get_fed_sentiment))
            .route("/btc", web::get().to(get_btc_sentiment))
            .route("/regime", web::get().to(get_regime))
    );
}
