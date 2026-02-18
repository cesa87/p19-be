//! MRATE API Endpoints
//! 
//! GET /api/mrate/regime - Current MRATE output
//! GET /api/mrate/history - Historical snapshots (if persistence enabled)

use actix_web::{web, HttpResponse, Responder};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::mrate::{MrateState, get_current_mrate, default_mrate_output, MrateOutput, Regime, DynamicThresholds, InstrumentScores, InstrumentScore};

/// Response for /api/mrate/regime
#[derive(Debug, Serialize)]
pub struct RegimeResponse {
    pub timestamp: String,
    pub regime: String,
    pub regime_description: String,
    pub liquidity_score: f64,
    pub risk_score: f64,
    pub uncertainty_score: f64,
    pub risk_multiplier: f64,
    pub regime_confidence: f64,
    pub strategy_weights: StrategyWeightsResponse,
    pub favored_instrument: String,
    pub instrument_scores: InstrumentScoresResponse,
    pub thresholds: ThresholdsResponse,
    pub liquidity_momentum: f64,
    pub risk_momentum: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<InputsResponse>,
}

/// Instrument-specific scores for multi-instrument trading
#[derive(Debug, Serialize)]
pub struct InstrumentScoresResponse {
    pub gold: InstrumentScoreResponse,
    pub bitcoin: InstrumentScoreResponse,
    pub eur_usd: InstrumentScoreResponse,
    pub usd_jpy: InstrumentScoreResponse,
    pub wti_oil: InstrumentScoreResponse,
    pub natural_gas: InstrumentScoreResponse,
}

#[derive(Debug, Serialize)]
pub struct InstrumentScoreResponse {
    pub symbol: String,
    pub score: f64,
    pub recommendation: String,
    pub factors: Vec<String>,
    /// Per-instrument trend direction (e.g. "Up", "Down", "StrongDown")
    pub trend_direction: String,
    /// ADX value (trend strength 0-100)
    pub trend_strength: f64,
    /// Per-instrument price regime: TRENDING, RANGING, or TRANSITIONAL
    pub price_regime: String,
    /// Bidirectional trading bias from model: LONG, SHORT, or NEUTRAL
    pub trading_direction: String,
    /// Best strategy categories for this instrument's current conditions
    pub best_strategies: Vec<String>,
    /// Technical indicators
    pub technicals: TechnicalsResponse,
    /// D1 200 EMA value - used for direction decision
    pub ema_200: f64,
    /// D1 current price
    pub current_price: f64,
}

/// Per-instrument technical indicators for the frontend
#[derive(Debug, Serialize)]
pub struct TechnicalsResponse {
    pub rsi: f64,
    pub bollinger_percent_b: f64,
    pub macd_histogram: f64,
    pub plus_di: f64,
    pub minus_di: f64,
    pub stochastic_k: f64,
    pub momentum_score: f64,
    pub ema_50: f64,
    pub ema_200: f64,
    pub atr: f64,
    pub current_price: f64,
}

impl From<&InstrumentScore> for InstrumentScoreResponse {
    fn from(s: &InstrumentScore) -> Self {
        use crate::mrate::models::TradingDirection;
        
        // Use model-derived trading direction (from InstrumentTechnicals::derive_direction)
        let trading_direction = match s.trading_direction {
            TradingDirection::Long => "LONG",
            TradingDirection::Short => "SHORT",
            TradingDirection::Neutral => "NEUTRAL",
        };
        
        // Enhanced recommendation that includes direction
        let recommendation = match trading_direction {
            "LONG" => {
                if s.score >= 60.0 { "StrongBuy".to_string() }
                else { "Buy".to_string() }
            },
            "SHORT" => {
                if s.score <= 40.0 { "StrongSell".to_string() }
                else { "Sell".to_string() }
            },
            _ => format_recommendation(s.score),
        };
        
        Self {
            symbol: s.symbol.clone(),
            score: s.score,
            recommendation,
            factors: s.factors.clone(),
            trend_direction: format!("{:?}", s.trend_direction),
            trend_strength: s.trend_strength,
            price_regime: s.price_regime.as_str().to_string(),
            trading_direction: trading_direction.to_string(),
            best_strategies: s.best_strategies.iter().map(|c| format!("{:?}", c)).collect(),
            technicals: TechnicalsResponse {
                rsi: s.technicals.rsi,
                bollinger_percent_b: s.technicals.bollinger_percent_b,
                macd_histogram: s.technicals.macd_histogram,
                plus_di: s.technicals.plus_di,
                minus_di: s.technicals.minus_di,
                stochastic_k: s.technicals.stochastic_k,
                momentum_score: s.technicals.momentum_score,
                ema_50: s.technicals.ema_50,
                ema_200: s.technicals.ema_200,
                atr: s.technicals.atr,
                current_price: s.technicals.current_price,
            },
            // D1 EMA values for direction display
            ema_200: s.ema_200,
            current_price: s.current_price,
        }
    }
}

/// Format recommendation from score (original logic)
fn format_recommendation(score: f64) -> String {
    if score >= 75.0 { "StrongBuy".to_string() }
    else if score >= 60.0 { "Buy".to_string() }
    else if score >= 40.0 { "Neutral".to_string() }
    else if score >= 25.0 { "Avoid".to_string() }
    else { "StrongAvoid".to_string() }
}

impl From<&InstrumentScores> for InstrumentScoresResponse {
    fn from(scores: &InstrumentScores) -> Self {
        Self {
            gold: InstrumentScoreResponse::from(&scores.gold),
            bitcoin: InstrumentScoreResponse::from(&scores.bitcoin),
            eur_usd: InstrumentScoreResponse::from(&scores.eur_usd),
            usd_jpy: InstrumentScoreResponse::from(&scores.usd_jpy),
            wti_oil: InstrumentScoreResponse::from(&scores.wti_oil),
            natural_gas: InstrumentScoreResponse::from(&scores.natural_gas),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ThresholdsResponse {
    pub market_aggression: f64,
    pub entry_softening: f64,
    pub breakout_aggression: f64,
    pub mean_reversion_aggression: f64,
    pub stop_multiplier: f64,
    pub tp_multiplier: f64,
}

#[derive(Debug, Serialize)]
pub struct StrategyWeightsResponse {
    pub trend: f64,
    pub breakout: f64,
    pub mean_reversion: f64,
    pub liquidity_sweep: f64,
}

#[derive(Debug, Serialize)]
pub struct InputsResponse {
    // Polymarket
    pub rate_cut_probability: Option<f64>,
    pub inflation_expectations: Option<f64>,
    pub safe_haven_demand: Option<f64>,
    pub btc_sentiment: Option<f64>,
    pub fed_uncertainty: Option<f64>,
    
    // Market trends
    pub dxy_trend: Option<String>,
    pub sp500_trend: Option<String>,
    pub yield_10y_trend: Option<String>,
    
    // VIX
    pub vix_level: Option<f64>,
    
    // DXY (US Dollar Index)
    pub dxy_level: Option<f64>,
    
    // Fear & Greed Index
    pub fear_greed_index: Option<f64>,
    pub fear_greed_category: Option<String>,
    
    // Treasury yields (FRED)
    pub yield_10y: Option<f64>,
    pub yield_2y: Option<f64>,
    pub real_yield_10y: Option<f64>,
    
    // Crypto market
    pub stablecoin_dominance: Option<f64>,
    pub btc_exchange_netflow: Option<f64>,
    
    // Gold-specific
    pub atr_percentile: Option<f64>,
    pub high_impact_event: bool,
    
    // BTC-specific
    pub btc_trend: Option<String>,
    pub btc_atr_percentile: Option<f64>,
    
    // New sentiment feeds
    pub reddit_crypto_sentiment: Option<f64>,
    pub reddit_btc_sentiment: Option<f64>,
    pub reddit_gold_sentiment: Option<f64>,
    pub alt_fear_greed_index: Option<f64>,
}

impl From<MrateOutput> for RegimeResponse {
    fn from(output: MrateOutput) -> Self {
        Self {
            timestamp: output.timestamp.to_rfc3339(),
            regime: output.regime.as_str().to_string(),
            regime_description: output.regime.description().to_string(),
            liquidity_score: output.liquidity_score,
            risk_score: output.risk_score,
            uncertainty_score: output.uncertainty_score,
            risk_multiplier: output.risk_multiplier,
            regime_confidence: output.regime_confidence,
            strategy_weights: StrategyWeightsResponse {
                trend: output.strategy_weights.trend,
                breakout: output.strategy_weights.breakout,
                mean_reversion: output.strategy_weights.mean_reversion,
                liquidity_sweep: output.strategy_weights.liquidity_sweep,
            },
            favored_instrument: output.favored_instrument.as_str().to_string(),
            instrument_scores: InstrumentScoresResponse::from(&output.instrument_scores),
            thresholds: ThresholdsResponse {
                market_aggression: output.thresholds.market_aggression,
                entry_softening: output.thresholds.entry_softening,
                breakout_aggression: output.thresholds.breakout_aggression,
                mean_reversion_aggression: output.thresholds.mean_reversion_aggression,
                stop_multiplier: output.thresholds.stop_multiplier,
                tp_multiplier: output.thresholds.tp_multiplier,
            },
            liquidity_momentum: output.liquidity_momentum,
            risk_momentum: output.risk_momentum,
            inputs: output.inputs.map(|i| InputsResponse {
                rate_cut_probability: i.rate_cut_probability,
                inflation_expectations: i.inflation_expectations,
                safe_haven_demand: i.safe_haven_demand,
                btc_sentiment: i.btc_sentiment,
                fed_uncertainty: i.fed_uncertainty,
                dxy_trend: i.dxy_trend.map(|t| format!("{:?}", t)),
                sp500_trend: i.sp500_trend.map(|t| format!("{:?}", t)),
                yield_10y_trend: i.yield_10y_trend.map(|t| format!("{:?}", t)),
                vix_level: i.vix_level,
                dxy_level: i.dxy_level,
                fear_greed_index: i.fear_greed_index,
                fear_greed_category: i.fear_greed_category,
                yield_10y: i.yield_10y,
                yield_2y: i.yield_2y,
                real_yield_10y: i.real_yield_10y,
                stablecoin_dominance: i.stablecoin_dominance,
                btc_exchange_netflow: i.btc_exchange_netflow,
                atr_percentile: i.atr_percentile,
                high_impact_event: i.high_impact_event_within_24h,
                btc_trend: i.btc_trend.map(|t| format!("{:?}", t)),
                btc_atr_percentile: i.btc_atr_percentile,
                reddit_crypto_sentiment: i.reddit_crypto_sentiment,
                reddit_btc_sentiment: i.reddit_btc_sentiment,
                reddit_gold_sentiment: i.reddit_gold_sentiment,
                alt_fear_greed_index: i.alt_fear_greed_index,
            }),
        }
    }
}

/// GET /api/mrate/regime
/// Returns current MRATE regime and scores
pub async fn get_regime(
    state: web::Data<MrateState>,
) -> impl Responder {
    let output = get_current_mrate(&state)
        .await
        .unwrap_or_else(default_mrate_output);
    
    HttpResponse::Ok().json(RegimeResponse::from(output))
}

/// Query params for history endpoint
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Hours of history to fetch (default 24)
    pub hours: Option<i64>,
    /// Maximum number of records (default 100)
    pub limit: Option<i64>,
}

/// Response for /api/mrate/history
#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub snapshots: Vec<SnapshotResponse>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub timestamp: String,
    pub regime: String,
    pub liquidity_score: f32,
    pub risk_score: f32,
    pub uncertainty_score: f32,
    pub risk_multiplier: f32,
}

/// GET /api/mrate/history
/// Returns historical MRATE snapshots (if persistence enabled)
pub async fn get_history(
    pool: web::Data<sqlx::PgPool>,
    query: web::Query<HistoryQuery>,
) -> impl Responder {
    let hours = query.hours.unwrap_or(24);
    let limit = query.limit.unwrap_or(100);
    let since = Utc::now() - Duration::hours(hours);
    
    let snapshots = sqlx::query_as::<_, (chrono::DateTime<Utc>, String, f32, f32, f32, f32)>(
        r#"
        SELECT timestamp, regime, liquidity_score, risk_score, uncertainty_score, risk_multiplier
        FROM mrate_snapshots
        WHERE timestamp > $1
        ORDER BY timestamp DESC
        LIMIT $2
        "#
    )
    .bind(since)
    .bind(limit)
    .fetch_all(pool.get_ref())
    .await;
    
    match snapshots {
        Ok(rows) => {
            let snapshots: Vec<SnapshotResponse> = rows
                .into_iter()
                .map(|(timestamp, regime, liq, risk, unc, mult)| SnapshotResponse {
                    timestamp: timestamp.to_rfc3339(),
                    regime,
                    liquidity_score: liq,
                    risk_score: risk,
                    uncertainty_score: unc,
                    risk_multiplier: mult,
                })
                .collect();
            
            let count = snapshots.len();
            HttpResponse::Ok().json(HistoryResponse { snapshots, count })
        }
        Err(e) => {
            // Table might not exist if persistence disabled
            HttpResponse::Ok().json(HistoryResponse { 
                snapshots: vec![], 
                count: 0 
            })
        }
    }
}

/// GET /api/mrate/weights/:category
/// Check if a specific strategy category should trade
#[derive(Debug, Serialize)]
pub struct WeightCheckResponse {
    pub category: String,
    pub weight: f64,
    pub should_trade: bool,
    pub regime: String,
    pub risk_multiplier: f64,
}

pub async fn check_weight(
    state: web::Data<MrateState>,
    path: web::Path<String>,
) -> impl Responder {
    let category = path.into_inner();
    let output = get_current_mrate(&state)
        .await
        .unwrap_or_else(default_mrate_output);
    
    let weight = match category.as_str() {
        "trend" => output.strategy_weights.trend,
        "breakout" => output.strategy_weights.breakout,
        "mean_reversion" => output.strategy_weights.mean_reversion,
        "liquidity_sweep" => output.strategy_weights.liquidity_sweep,
        _ => return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid category. Use: trend, breakout, mean_reversion, liquidity_sweep"
        })),
    };
    
    HttpResponse::Ok().json(WeightCheckResponse {
        category,
        weight,
        should_trade: weight >= 0.4,
        regime: output.regime.as_str().to_string(),
        risk_multiplier: output.risk_multiplier,
    })
}

/// Alert types for MRATE notifications
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    FavorableRegime,
    StrategyUnassigned,
    BotNotRunning,
    RegimeChange,
}

#[derive(Debug, Serialize)]
pub struct MrateAlert {
    pub alert_type: AlertType,
    pub severity: String,  // "info", "warning", "success"
    pub title: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AlertsResponse {
    pub regime: String,
    pub favored_instrument: String,
    pub alerts: Vec<MrateAlert>,
}

/// GET /api/mrate/alerts
/// Returns actionable alerts based on current MRATE state
pub async fn get_alerts(
    state: web::Data<MrateState>,
    pool: web::Data<sqlx::PgPool>,
) -> impl Responder {
    let output = get_current_mrate(&state)
        .await
        .unwrap_or_else(default_mrate_output);
    
    let mut alerts = Vec::new();
    
    // Determine which categories are favored (weight >= 70%)
    let favored_categories: Vec<(&str, f64)> = [
        ("trend", output.strategy_weights.trend),
        ("breakout", output.strategy_weights.breakout),
        ("mean_reversion", output.strategy_weights.mean_reversion),
        ("liquidity_sweep", output.strategy_weights.liquidity_sweep),
    ].into_iter()
    .filter(|(_, w)| *w >= 0.7)
    .collect();
    
    // Alert: Favorable regime detected
    if !favored_categories.is_empty() {
        let categories_str = favored_categories.iter()
            .map(|(c, w)| format!("{} ({:.0}%)", c, w * 100.0))
            .collect::<Vec<_>>()
            .join(", ");
        
        alerts.push(MrateAlert {
            alert_type: AlertType::FavorableRegime,
            severity: "success".to_string(),
            title: format!("{} conditions detected", output.regime.as_str().replace("_", " ")),
            message: format!(
                "Favorable for: {}. {} is recommended.",
                categories_str,
                output.favored_instrument.as_str()
            ),
            action: None,
            related_id: None,
        });
    }
    
    // Check for strategies without MRATE categories
    let unassigned_strategies: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM strategies WHERE mrate_category IS NULL"
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();
    
    if !unassigned_strategies.is_empty() && !favored_categories.is_empty() {
        for (id, name) in &unassigned_strategies {
            alerts.push(MrateAlert {
                alert_type: AlertType::StrategyUnassigned,
                severity: "warning".to_string(),
                title: format!("Strategy '{}' missing MRATE category", name),
                message: "This strategy won't benefit from regime-aware trading or threshold relaxation.".to_string(),
                action: Some("Assign an MRATE category in Strategy settings".to_string()),
                related_id: Some(id.to_string()),
            });
        }
    }
    
    // Check for stopped bots that could be trading
    if !favored_categories.is_empty() {
        // Find bots that are not active but have strategies with favorable categories
        let stopped_bots: Vec<(uuid::Uuid, String, String)> = sqlx::query_as(
            r#"
            SELECT b.id, b.name, s.mrate_category
            FROM bots b
            JOIN strategies s ON b.strategy_id = s.id
            WHERE b.is_active = false 
            AND s.mrate_category IS NOT NULL
            "#
        )
        .fetch_all(pool.get_ref())
        .await
        .unwrap_or_default();
        
        for (id, name, category) in stopped_bots {
            // Check if this category is favored
            let weight = match category.as_str() {
                "trend" => output.strategy_weights.trend,
                "breakout" => output.strategy_weights.breakout,
                "mean_reversion" => output.strategy_weights.mean_reversion,
                "liquidity_sweep" => output.strategy_weights.liquidity_sweep,
                _ => 0.0,
            };
            
            if weight >= 0.7 {
                alerts.push(MrateAlert {
                    alert_type: AlertType::BotNotRunning,
                    severity: "info".to_string(),
                    title: format!("Bot '{}' could be trading", name),
                    message: format!(
                        "This {} bot has {:.0}% weight in current {} regime.",
                        category, weight * 100.0, output.regime.as_str()
                    ),
                    action: Some("Start bot from Bot Config".to_string()),
                    related_id: Some(id.to_string()),
                });
            }
        }
    }
    
    HttpResponse::Ok().json(AlertsResponse {
        regime: output.regime.as_str().to_string(),
        favored_instrument: output.favored_instrument.as_str().to_string(),
        alerts,
    })
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub overall_healthy: bool,
    pub degraded: bool,
    pub feeds: FeedHealthResponse,
    pub healthy_feed_count: usize,
    pub total_feeds: usize,
    pub status_summary: String,
}

#[derive(Debug, Serialize)]
pub struct FeedHealthResponse {
    pub polymarket: FeedStatus,
    pub vix: FeedStatus,
    pub fred: FeedStatus,
    pub oanda: FeedStatus,
    pub calendar: FeedStatus,
    pub dxy: FeedStatus,
    pub fear_greed: FeedStatus,
}

#[derive(Debug, Serialize)]
pub struct FeedStatus {
    pub ok: bool,
    pub last_success: Option<String>,
    pub minutes_since_success: Option<i64>,
}

/// GET /api/mrate/health
/// Returns data feed health status
pub async fn get_health(
    state: web::Data<MrateState>,
) -> impl Responder {
    let output = get_current_mrate(&state)
        .await
        .unwrap_or_else(default_mrate_output);
    
    let health = output.data_health;
    let now = Utc::now();
    let to_feed_status = |ok: bool, last: Option<chrono::DateTime<Utc>>| FeedStatus {
        ok,
        last_success: last.map(|t| t.to_rfc3339()),
        minutes_since_success: last.map(|t| (now - t).num_minutes()),
    };
    
    HttpResponse::Ok().json(HealthResponse {
        overall_healthy: !health.is_degraded(),
        degraded: health.is_degraded(),
        feeds: FeedHealthResponse {
            polymarket: to_feed_status(health.polymarket_ok, health.last_polymarket_success),
            vix: to_feed_status(health.vix_ok, health.last_vix_success),
            fred: to_feed_status(health.fred_ok, health.last_fred_success),
            oanda: to_feed_status(health.oanda_ok, health.last_oanda_success),
            calendar: to_feed_status(health.calendar_ok, None),
            dxy: to_feed_status(health.dxy_ok, health.last_dxy_success),
            fear_greed: to_feed_status(health.fear_greed_ok, health.last_fear_greed_success),
        },
        healthy_feed_count: health.healthy_feed_count(),
        total_feeds: 7,
        status_summary: health.status_summary(),
    })
}

/// POST /api/mrate/refresh
/// Force immediate MRATE refresh (uses shared engine to preserve hysteresis state)
pub async fn force_refresh(
    state: web::Data<MrateState>,
    engine: web::Data<crate::mrate::SharedMrateEngine>,
    config: web::Data<crate::config::Config>,
) -> impl Responder {
    use crate::broker::OandaClient;
    
    tracing::info!("🔄 Manual MRATE refresh requested");
    
    // Create OANDA client if credentials available
    let oanda = if let (Some(token), Some(account)) = (&config.oanda_api_token, &config.oanda_account_id) {
        OandaClient::new(token, account, config.oanda_practice).ok()
    } else {
        None
    };
    
    // Run engine calculation using SHARED engine (preserves hysteresis state)
    let output = engine.calculate(oanda.as_ref()).await;
    
    // Update shared state
    {
        let mut state_guard = state.write().await;
        *state_guard = Some(output.clone());
    }
    
    tracing::info!("✅ MRATE manually refreshed: {} (DXY: {}, F&G: {})", 
        output.regime.as_str(),
        if output.data_health.dxy_ok { "OK" } else { "FAIL" },
        if output.data_health.fear_greed_ok { "OK" } else { "FAIL" }
    );
    
    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "MRATE refresh complete",
        "regime": output.regime.as_str(),
        "dxy_ok": output.data_health.dxy_ok,
        "fear_greed_ok": output.data_health.fear_greed_ok,
        "healthy_feeds": output.data_health.healthy_feed_count()
    }))
}

/// GET /api/mrate/activities
/// Returns recent MRATE activity log for transparency
#[derive(Debug, Deserialize)]
pub struct ActivitiesQuery {
    /// Maximum number of activities (default 50)
    pub limit: Option<i32>,
    /// Filter by activity type (optional)
    pub activity_type: Option<String>,
}

pub async fn get_activities(
    pool: web::Data<sqlx::PgPool>,
    query: web::Query<ActivitiesQuery>,
) -> impl Responder {
    use crate::mrate::activity_log::{get_recent_activities, get_activities_by_type};
    
    let limit = query.limit.unwrap_or(50);
    
    let activities = if let Some(ref activity_type) = query.activity_type {
        get_activities_by_type(pool.get_ref(), activity_type, limit).await
    } else {
        get_recent_activities(pool.get_ref(), limit).await
    };
    
    match activities {
        Ok(activities) => HttpResponse::Ok().json(serde_json::json!({
            "activities": activities,
            "count": activities.len()
        })),
        Err(e) => {
            tracing::warn!("Failed to fetch MRATE activities: {}", e);
            HttpResponse::Ok().json(serde_json::json!({
                "activities": [],
                "count": 0
            }))
        }
    }
}

/// Configure MRATE API routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/mrate")
            .route("/regime", web::get().to(get_regime))
            .route("/history", web::get().to(get_history))
            .route("/weights/{category}", web::get().to(check_weight))
            .route("/alerts", web::get().to(get_alerts))
            .route("/health", web::get().to(get_health))
            .route("/refresh", web::post().to(force_refresh))
            .route("/activities", web::get().to(get_activities))
    );
}
