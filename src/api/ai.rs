//! AI Brain API - Endpoints for ML Systems Dashboard
//! 
//! Exposes real-time ML decision data, learning stats, predictions

use actix_web::{get, web, HttpResponse, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::analytics::{
    ensemble::EnsembleEngine,
    correlation::CorrelationEngine,
    evolution::EvolutionEngine,
    trailing_stop::TrailingStopManager,
    trade_analyzer::{TradeAnalyzer, TradeAnalysis, TradeInsights, TradeRecommendation},
};
use crate::mrate::advisor::{MrateAdvisor, AdvisorOutput, Contradiction, Suggestion, BlockedOpportunity, PositionInfo};
use crate::mrate::predictor::RegimePredictor;
use crate::feeds::event_learner::EventLearner;
use crate::broker::oanda::OandaClient;

/// GET /api/ai/ensemble - Cluster performance stats
#[derive(Debug, Serialize)]
pub struct EnsembleResponse {
    pub clusters: Vec<ClusterInfo>,
    pub total_patterns: i32,
    pub active_clusters: i32,
}

#[derive(Debug, Serialize)]
pub struct ClusterInfo {
    pub cluster_key: String,
    pub pattern_hash: i64,
    pub trades_count: i32,
    pub wins: i32,
    pub losses: i32,
    pub win_rate: f64,
    pub confidence_multiplier: f64,
    pub last_outcome: Option<String>,
}

#[get("/ai/ensemble")]
pub async fn get_ensemble_stats(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    let _engine = EnsembleEngine::new(pool.get_ref().clone());
    
    // Fetch top 20 clusters by recent activity with aggregated stats
    let clusters = sqlx::query!(
        r#"
        SELECT 
            cluster_id,
            pattern_hash,
            COUNT(*) as trades_count,
            SUM(CASE WHEN pnl > 0 THEN 1 ELSE 0 END)::int as wins,
            SUM(CASE WHEN pnl <= 0 THEN 1 ELSE 0 END)::int as losses,
            AVG(CASE WHEN pnl > 0 THEN 1.0 ELSE 0.0 END) as win_rate,
            (ARRAY_AGG(outcome ORDER BY created_at DESC))[1] as last_outcome,
            MAX(created_at) as last_updated
        FROM cross_bot_learnings
        WHERE created_at > NOW() - INTERVAL '30 days'
        GROUP BY cluster_id, pattern_hash
        ORDER BY last_updated DESC
        LIMIT 20
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    let cluster_infos: Vec<ClusterInfo> = clusters
        .into_iter()
        .map(|row| {
            let trades_count = row.trades_count.unwrap_or(0) as i32;
            let wins = row.wins.unwrap_or(0);
            let losses = row.losses.unwrap_or(0);
            let win_rate = row.win_rate.map(|v| v.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);
            
            // Calculate confidence multiplier
            let confidence_multiplier = if trades_count >= 3 {
                if win_rate > 0.60 {
                    1.15
                } else if win_rate < 0.40 && trades_count >= 5 {
                    0.85
                } else {
                    1.0
                }
            } else {
                1.0
            };
            
            ClusterInfo {
                cluster_key: row.cluster_id,
                pattern_hash: row.pattern_hash.parse().unwrap_or(0),
                trades_count,
                wins,
                losses,
                win_rate,
                confidence_multiplier,
                last_outcome: row.last_outcome,
            }
        })
        .collect();
    
    let total_patterns = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM cross_bot_learnings"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0);
    
    let active_clusters = cluster_infos.len() as i32;
    
    Ok(HttpResponse::Ok().json(EnsembleResponse {
        clusters: cluster_infos,
        total_patterns: total_patterns as i32,
        active_clusters,
    }))
}

/// GET /api/ai/regime-prediction - Regime transition prediction
#[derive(Debug, Serialize)]
pub struct RegimePredictionResponse {
    pub has_prediction: bool,
    pub current_regime: Option<String>,
    pub predicted_regime: Option<String>,
    pub probability: f64,
    pub time_horizon_hours: i32,
    pub confidence: String,
    pub reasoning: String,
    pub factors: Vec<PredictionFactor>,
    pub upcoming_events: Vec<UpcomingEvent>,
    pub momentum_analysis: MomentumAnalysis,
    pub regime_stability: RegimeStability,
}

#[derive(Debug, Serialize)]
pub struct PredictionFactor {
    pub factor: String,
    pub impact: String, // "bullish", "bearish", "neutral"
    pub weight: f64,
}

#[derive(Debug, Serialize)]
pub struct UpcomingEvent {
    pub name: String,
    pub hours_until: f64,
    pub expected_impact: String,
    pub regime_implication: String,
}

#[derive(Debug, Serialize)]
pub struct MomentumAnalysis {
    pub liquidity_momentum: f64,
    pub risk_momentum: f64,
    pub liquidity_direction: String,
    pub risk_direction: String,
    pub momentum_alignment: String,
}

#[derive(Debug, Serialize)]
pub struct RegimeStability {
    pub current_regime: String,
    pub minutes_in_regime: i64,
    pub regime_confidence: f64,
    pub change_pending: bool,
    pub proposed_regime: Option<String>,
}

#[get("/ai/regime-prediction")]
pub async fn get_regime_prediction(
    pool: web::Data<PgPool>,
    mrate_state: web::Data<crate::mrate::MrateState>,
) -> Result<HttpResponse> {
    use crate::mrate::models::Regime;
    
    // Get current MRATE state
    let mrate_guard = mrate_state.read().await;
    let mrate = mrate_guard.as_ref().cloned();
    drop(mrate_guard);
    
    let Some(mrate) = mrate else {
        return Ok(HttpResponse::Ok().json(RegimePredictionResponse {
            has_prediction: false,
            current_regime: None,
            predicted_regime: None,
            probability: 0.0,
            time_horizon_hours: 0,
            confidence: "none".to_string(),
            reasoning: "MRATE not yet initialized - waiting for first calculation".to_string(),
            factors: vec![],
            upcoming_events: vec![],
            momentum_analysis: MomentumAnalysis {
                liquidity_momentum: 0.0,
                risk_momentum: 0.0,
                liquidity_direction: "unknown".to_string(),
                risk_direction: "unknown".to_string(),
                momentum_alignment: "unknown".to_string(),
            },
            regime_stability: RegimeStability {
                current_regime: "UNKNOWN".to_string(),
                minutes_in_regime: 0,
                regime_confidence: 0.0,
                change_pending: false,
                proposed_regime: None,
            },
        }));
    };
    
    let inputs = mrate.inputs.as_ref();
    let current_regime = mrate.regime;
    
    // Build momentum analysis
    let liq_dir = if mrate.liquidity_momentum > 5.0 { "improving" }
        else if mrate.liquidity_momentum < -5.0 { "deteriorating" }
        else { "stable" };
    let risk_dir = if mrate.risk_momentum > 5.0 { "increasing" }
        else if mrate.risk_momentum < -5.0 { "decreasing" }
        else { "stable" };
    let alignment = if mrate.liquidity_momentum > 0.0 && mrate.risk_momentum < 0.0 {
        "risk-on favorable"
    } else if mrate.liquidity_momentum < 0.0 && mrate.risk_momentum > 0.0 {
        "risk-off favorable"
    } else if mrate.liquidity_momentum.abs() < 3.0 && mrate.risk_momentum.abs() < 3.0 {
        "neutral/sideways"
    } else {
        "mixed signals"
    };
    
    let momentum_analysis = MomentumAnalysis {
        liquidity_momentum: mrate.liquidity_momentum,
        risk_momentum: mrate.risk_momentum,
        liquidity_direction: liq_dir.to_string(),
        risk_direction: risk_dir.to_string(),
        momentum_alignment: alignment.to_string(),
    };
    
    // Build regime stability info
    let regime_stability = RegimeStability {
        current_regime: current_regime.as_str().to_string(),
        minutes_in_regime: 0, // Would need regime state tracking
        regime_confidence: mrate.regime_confidence,
        change_pending: mrate.regime_change_pending,
        proposed_regime: if mrate.regime_change_pending {
            Some(mrate.proposed_regime.as_str().to_string())
        } else {
            None
        },
    };
    
    // Gather prediction factors
    let mut factors = Vec::new();
    
    // Liquidity momentum factor
    if mrate.liquidity_momentum.abs() > 5.0 {
        factors.push(PredictionFactor {
            factor: format!("Liquidity momentum: {:.1}", mrate.liquidity_momentum),
            impact: if mrate.liquidity_momentum > 0.0 { "bullish" } else { "bearish" }.to_string(),
            weight: (mrate.liquidity_momentum.abs() / 20.0).min(1.0),
        });
    }
    
    // Risk momentum factor
    if mrate.risk_momentum.abs() > 5.0 {
        factors.push(PredictionFactor {
            factor: format!("Risk momentum: {:.1}", mrate.risk_momentum),
            impact: if mrate.risk_momentum > 0.0 { "bearish" } else { "bullish" }.to_string(),
            weight: (mrate.risk_momentum.abs() / 20.0).min(1.0),
        });
    }
    
    // VIX level factor
    if let Some(vix) = inputs.and_then(|i| i.vix_level) {
        let (impact, desc) = if vix > 25.0 {
            ("bearish", "VIX elevated - fear in markets")
        } else if vix < 15.0 {
            ("bullish", "VIX low - complacency/calm")
        } else {
            ("neutral", "VIX normal range")
        };
        factors.push(PredictionFactor {
            factor: format!("{} (VIX: {:.1})", desc, vix),
            impact: impact.to_string(),
            weight: if vix > 25.0 || vix < 15.0 { 0.7 } else { 0.3 },
        });
    }
    
    // Fear & Greed factor
    if let Some(fg) = inputs.and_then(|i| i.fear_greed_index) {
        let (impact, desc) = if fg < 25.0 {
            ("bullish", "Extreme Fear - contrarian buy signal")
        } else if fg > 75.0 {
            ("bearish", "Extreme Greed - contrarian sell signal")
        } else if fg < 40.0 {
            ("neutral", "Fear zone")
        } else if fg > 60.0 {
            ("neutral", "Greed zone")
        } else {
            ("neutral", "Neutral sentiment")
        };
        factors.push(PredictionFactor {
            factor: format!("{} (F&G: {:.0})", desc, fg),
            impact: impact.to_string(),
            weight: if fg < 25.0 || fg > 75.0 { 0.8 } else { 0.4 },
        });
    }
    
    // Build upcoming events list
    let mut upcoming_events = Vec::new();
    if let Some(hours) = inputs.and_then(|i| i.hours_to_next_high_impact) {
        if hours < 48.0 {
            let event_weight = inputs.map(|i| i.next_event_weight).unwrap_or(1.0);
            let impact = if event_weight > 1.3 { "HIGH" } else if event_weight > 1.1 { "MEDIUM" } else { "LOW" };
            let implication = if hours < 4.0 {
                "Expect volatility spike - regime change possible"
            } else if hours < 12.0 {
                "Positioning ahead of event - watch for breakouts"
            } else {
                "Event on horizon - monitor for early moves"
            };
            upcoming_events.push(UpcomingEvent {
                name: "High-Impact Economic Event".to_string(),
                hours_until: hours,
                expected_impact: impact.to_string(),
                regime_implication: implication.to_string(),
            });
        }
    }
    
    // Use the predictor for ML-based prediction
    let predictor = RegimePredictor::new(pool.get_ref().clone());
    let prediction_result = predictor.predict(
        current_regime,
        mrate.liquidity_score,
        mrate.risk_score,
        mrate.uncertainty_score,
        mrate.liquidity_momentum,
        mrate.risk_momentum,
        inputs.and_then(|i| i.vix_level),
        inputs.and_then(|i| i.fear_greed_index),
    ).await;
    
    // Build final response
    let (has_prediction, predicted_regime, probability, horizon, confidence, reasoning) = match prediction_result {
        Ok(Some(pred)) => (
            true,
            Some(pred.predicted_regime.as_str().to_string()),
            pred.probability,
            pred.horizon_hours,
            pred.confidence,
            pred.reasoning.join("; "),
        ),
        Ok(None) => {
            // No ML prediction, but we can still provide analysis
            let reasoning = if mrate.regime_change_pending {
                format!("Regime change to {} pending - waiting for confirmation", mrate.proposed_regime.as_str())
            } else if mrate.regime_confidence > 0.8 {
                format!("Current {} regime is stable (confidence: {:.0}%)", current_regime.as_str(), mrate.regime_confidence * 100.0)
            } else {
                "Monitoring for regime transition signals".to_string()
            };
            (false, None, 0.0, 0, "monitoring".to_string(), reasoning)
        },
        Err(e) => {
            tracing::warn!("Regime prediction error: {}", e);
            (false, None, 0.0, 0, "error".to_string(), "Prediction model error - using heuristics".to_string())
        }
    };
    
    Ok(HttpResponse::Ok().json(RegimePredictionResponse {
        has_prediction,
        current_regime: Some(current_regime.as_str().to_string()),
        predicted_regime,
        probability,
        time_horizon_hours: horizon,
        confidence,
        reasoning,
        factors,
        upcoming_events,
        momentum_analysis,
        regime_stability,
    }))
}

/// GET /api/ai/evolution - Parameter evolution stats
#[derive(Debug, Serialize)]
pub struct EvolutionResponse {
    pub current_generation: i32,
    pub total_variants: i32,
    pub testing_variants: i32,
    pub promoted_variants: i32,
    pub retired_variants: i32,
    pub top_variants: Vec<VariantInfo>,
}

#[derive(Debug, Serialize)]
pub struct VariantInfo {
    pub id: Uuid,
    pub parent_bot_name: String,
    pub variant_name: String,
    pub generation: i32,
    pub status: String,
    pub trades_count: i32,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_pnl: f64,
}

#[get("/ai/evolution")]
pub async fn get_evolution_stats(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    let current_generation = sqlx::query_scalar!(
        "SELECT COALESCE(MAX(generation), 0) FROM bot_variants"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0);
    
    let total_variants = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_variants"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0) as i32;
    
    let testing_variants = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_variants WHERE status = 'testing'"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0) as i32;
    
    let promoted_variants = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_variants WHERE status = 'promoted'"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0) as i32;
    
    let retired_variants = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_variants WHERE status = 'retired'"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0) as i32;
    
    // Get top 10 variants by performance
    let top_variants = sqlx::query!(
        r#"
        SELECT 
            bv.id,
            b.name as parent_bot_name,
            bv.variant_name,
            bv.generation,
            bv.status,
            bv.trades_count,
            bv.win_rate,
            bv.profit_factor,
            bv.total_pnl
        FROM bot_variants bv
        JOIN bots b ON bv.parent_bot_id = b.id
        WHERE bv.trades_count >= 10
        ORDER BY bv.profit_factor DESC, bv.win_rate DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    let variant_infos: Vec<VariantInfo> = top_variants
        .into_iter()
        .map(|row| VariantInfo {
            id: row.id,
            parent_bot_name: row.parent_bot_name,
            variant_name: row.variant_name,
            generation: row.generation,
            status: row.status,
            trades_count: row.trades_count,
            win_rate: row.win_rate.unwrap_or(0.0) as f64,
            profit_factor: row.profit_factor.unwrap_or(0.0) as f64,
            total_pnl: row.total_pnl as f64,
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(EvolutionResponse {
        current_generation,
        total_variants,
        testing_variants,
        promoted_variants,
        retired_variants,
        top_variants: variant_infos,
    }))
}

/// GET /api/ai/correlation - Instrument correlation matrix
#[derive(Debug, Serialize)]
pub struct CorrelationResponse {
    pub matrix: Vec<CorrelationRow>,
    pub last_updated: Option<String>,
    pub portfolio_correlation: f64,
}

#[derive(Debug, Serialize)]
pub struct CorrelationRow {
    pub instrument: String,
    pub correlations: Vec<f64>,
}

#[get("/ai/correlation")]
pub async fn get_correlation_matrix(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    let analyzer = CorrelationEngine::new(pool.get_ref().clone());
    
    let instruments = vec![
        "XAU_USD", "BTC_USD", "EUR_USD", "USD_JPY", "WTICO_USD", "NATGAS_USD"
    ];
    
    // Build correlation matrix
    let mut matrix = Vec::new();
    
    for &instr1 in &instruments {
        let mut correlations = Vec::new();
        
        for &instr2 in &instruments {
            let corr = sqlx::query_scalar!(
                r#"
                SELECT correlation
                FROM instrument_correlations
                WHERE instrument_a = $1 AND instrument_b = $2
                ORDER BY last_updated DESC
                LIMIT 1
                "#,
                instr1,
                instr2
            )
            .fetch_optional(pool.get_ref())
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?
            .unwrap_or(if instr1 == instr2 { 1.0 } else { 0.0 }) as f64;
            
            correlations.push(corr);
        }
        
        matrix.push(CorrelationRow {
            instrument: instr1.to_string(),
            correlations,
        });
    }
    
    // Get last update time
    let last_updated = sqlx::query_scalar!(
        r#"
        SELECT MAX(last_updated)::text
        FROM instrument_correlations
        "#
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    // Calculate portfolio correlation (average of all non-diagonal correlations)
    let mut sum = 0.0;
    let mut count = 0;
    for row in &matrix {
        for (i, &corr) in row.correlations.iter().enumerate() {
            if row.instrument != instruments[i] {
                sum += corr.abs();
                count += 1;
            }
        }
    }
    let portfolio_correlation = if count > 0 { sum / count as f64 } else { 0.0 };
    
    Ok(HttpResponse::Ok().json(CorrelationResponse {
        matrix,
        last_updated,
        portfolio_correlation,
    }))
}

/// GET /api/ai/events - Learned event impacts
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub high_impact_events: Vec<EventInfo>,
    pub recent_learnings: Vec<LearningInfo>,
    pub total_events_tracked: i32,
}

#[derive(Debug, Serialize)]
pub struct EventInfo {
    pub event_name: String,
    pub category: String,
    pub avg_impact_score: f64,
    pub observations: i32,
    pub last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LearningInfo {
    pub event_name: String,
    pub instrument: String,
    pub timestamp: String,
    pub impact_score: f64,
    pub volatility_spike: f64,
    pub price_move_pct: f64,
}

#[get("/ai/events")]
pub async fn get_event_impacts(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    // Get high impact events (score >= 7)
    let high_impact = sqlx::query!(
        r#"
        SELECT 
            event_name,
            event_category as category,
            impact_score,
            sample_size,
            last_occurrence::text as last_seen
        FROM event_impacts
        WHERE impact_score >= 7
        ORDER BY impact_score DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    let event_infos: Vec<EventInfo> = high_impact
        .into_iter()
        .map(|row| EventInfo {
            event_name: row.event_name,
            category: row.category.unwrap_or_else(|| "Unknown".to_string()),
            avg_impact_score: row.impact_score as f64,
            observations: row.sample_size,
            last_seen: row.last_seen,
        })
        .collect();
    
    // Get recent learnings (simplified - just recent events)
    let recent = sqlx::query!(
        r#"
        SELECT 
            event_name,
            impact_score,
            avg_volatility_spike,
            avg_price_move_pct,
            last_occurrence::text
        FROM event_impacts
        WHERE last_occurrence > NOW() - INTERVAL '7 days'
        ORDER BY last_occurrence DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();
    
    let learning_infos: Vec<LearningInfo> = recent
        .into_iter()
        .map(|row| LearningInfo {
            event_name: row.event_name,
            instrument: "Multi".to_string(),  // Event impacts are cross-instrument
            timestamp: row.last_occurrence.unwrap_or_default(),
            impact_score: row.impact_score as f64,
            volatility_spike: row.avg_volatility_spike as f64,
            price_move_pct: row.avg_price_move_pct.unwrap_or(0.0) as f64,
        })
        .collect();
    
    let total_events = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM event_impacts"
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .unwrap_or(0) as i32;
    
    Ok(HttpResponse::Ok().json(EventsResponse {
        high_impact_events: event_infos,
        recent_learnings: learning_infos,
        total_events_tracked: total_events,
    }))
}

/// GET /api/ai/sentiment - Current sentiment signals
#[derive(Debug, Serialize)]
pub struct SentimentResponse {
    pub fear_greed_index: i32,
    pub fear_greed_label: String,
    pub reddit_crypto_sentiment: f64,
    pub reddit_btc_sentiment: f64,
    pub reddit_gold_sentiment: f64,
    pub contrarian_signal: String,
    pub sentiment_alignment: String,
}

#[get("/ai/sentiment")]
pub async fn get_sentiment_signals(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    // Get latest sentiment from sentiment_signals table
    let latest = sqlx::query!(
        r#"
        SELECT 
            fear_greed_index,
            reddit_crypto,
            reddit_btc,
            reddit_gold
        FROM sentiment_signals
        ORDER BY timestamp DESC
        LIMIT 1
        "#
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    let (fg_idx, reddit_crypto, reddit_btc, reddit_gold) = if let Some(row) = latest {
        (
            row.fear_greed_index.unwrap_or(50.0) as i32,
            row.reddit_crypto.unwrap_or(0.5) as f64,
            row.reddit_btc.unwrap_or(0.5) as f64,
            row.reddit_gold.unwrap_or(0.5) as f64,
        )
    } else {
        (50, 0.5, 0.5, 0.5)
    };
    
    let fg_label = match fg_idx {
        0..=20 => "Extreme Fear",
        21..=40 => "Fear",
        41..=60 => "Neutral",
        61..=80 => "Greed",
        _ => "Extreme Greed",
    };
    
    let contrarian_signal = if fg_idx < 20 {
        "LONG opportunity (extreme fear)"
    } else if fg_idx > 80 {
        "SHORT opportunity (extreme greed)"
    } else {
        "No strong contrarian signal"
    };
    
    let avg_reddit = (reddit_crypto + reddit_btc + reddit_gold) / 3.0;
    let sentiment_alignment = if avg_reddit > 0.6 && fg_idx > 60 {
        "Aligned bullish (caution)"
    } else if avg_reddit < 0.4 && fg_idx < 40 {
        "Aligned bearish (opportunity)"
    } else {
        "Mixed signals"
    };
    
    Ok(HttpResponse::Ok().json(SentimentResponse {
        fear_greed_index: fg_idx,
        fear_greed_label: fg_label.to_string(),
        reddit_crypto_sentiment: reddit_crypto,
        reddit_btc_sentiment: reddit_btc,
        reddit_gold_sentiment: reddit_gold,
        contrarian_signal: contrarian_signal.to_string(),
        sentiment_alignment: sentiment_alignment.to_string(),
    }))
}

/// GET /api/ai/trailing-stops - Active trailing stops
#[derive(Debug, Serialize)]
pub struct TrailingStopsResponse {
    pub active_trails: Vec<TrailInfo>,
    pub total_active: i32,
}

#[derive(Debug, Serialize)]
pub struct TrailInfo {
    pub position_id: Uuid,
    pub instrument: String,
    pub direction: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub current_stop: f64,
    pub current_tp: f64,
    pub trail_distance_atr: f64,
    pub profit_pct: f64,
    pub tp_extended: bool,
    pub regime_adjustment: String,
    pub correlation_adjustment: f64,
    pub sentiment_adjustment: f64,
    pub last_updated: String,
}

#[get("/ai/trailing-stops")]
pub async fn get_trailing_stops(pool: web::Data<PgPool>) -> Result<HttpResponse> {
    let manager = TrailingStopManager::new(pool.get_ref().clone(), None);
    
    let states = manager
        .get_active_trails()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    
    let trail_infos: Vec<TrailInfo> = states
        .into_iter()
        .map(|state| {
            // Calculate current profit percentage
            let profit_pct = if state.direction == "long" {
                ((state.highest_price - state.entry_price) / state.entry_price) * 100.0
            } else {
                ((state.entry_price - state.lowest_price) / state.entry_price) * 100.0
            };
            
            TrailInfo {
                position_id: state.position_id,
                instrument: state.instrument,
                direction: state.direction.clone(),
                entry_price: state.entry_price,
                current_price: if state.direction == "long" {
                    state.highest_price
                } else {
                    state.lowest_price
                },
                current_stop: state.current_stop,
                current_tp: state.current_tp,
                trail_distance_atr: state.trail_distance_atr,
                profit_pct,
                tp_extended: state.tp_extended,
                regime_adjustment: state.regime_adjustment,
                correlation_adjustment: state.correlation_adjustment,
                sentiment_adjustment: state.sentiment_adjustment,
                last_updated: state.last_updated.to_rfc3339(),
            }
        })
        .collect();
    
    let total_active = trail_infos.len() as i32;
    
    Ok(HttpResponse::Ok().json(TrailingStopsResponse {
        active_trails: trail_infos,
        total_active,
    }))
}

/// GET /api/ai/advisory/:instrument/:direction - Trading advisory
#[derive(Debug, Serialize)]
pub struct AdvisoryResponse {
    pub instrument: String,
    pub direction: String,
    pub overall_recommendation: String,  // "START", "CAUTION", "SKIP"
    pub confidence_score: f64,           // 0.0-1.0
    pub position_size_multiplier: f64,   // 0.5-1.5
    pub warnings: Vec<AdvisoryMessage>,
    pub recommendations: Vec<AdvisoryMessage>,
    pub regime_analysis: Option<String>,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
pub struct AdvisoryMessage {
    pub severity: String,  // "high", "medium", "low"
    pub icon: String,
    pub message: String,
    pub source: String,    // "regime", "ensemble", "correlation", "sentiment"
}

#[get("/ai/advisory/{instrument}/{direction}")]
pub async fn get_trading_advisory(
    path: web::Path<(String, String)>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    let (instrument, direction) = path.into_inner();
    
    let mut warnings = Vec::new();
    let mut recommendations = Vec::new();
    let mut confidence_score: f64 = 1.0;
    let mut size_multiplier: f64 = 1.0;
    
    // 1. REGIME PREDICTION CHECK - Skip for now (requires MRATE integration)
    let mut regime_analysis: Option<String> = None;
    
    // 2. CORRELATION CHECK
    let corr_engine = CorrelationEngine::new(pool.get_ref().clone());
    // Check correlation with empty positions (just return 0 for now - integration point for orchestrator)
    if let Ok(corr_score) = corr_engine.get_correlation(&instrument, &instrument).await {
        if corr_score > 0.7 {
            warnings.push(AdvisoryMessage {
                severity: "medium".to_string(),
                icon: "🔗".to_string(),
                message: format!(
                    "High portfolio correlation ({:.2}) - Reduce position size by {}%",
                    corr_score,
                    ((1.0 - corr_score) * 100.0) as i32
                ),
                source: "correlation".to_string(),
            });
            size_multiplier *= 0.7;
        }
    }
    
    // 3. SENTIMENT CHECK
    let latest_mrate = sqlx::query!(
        r#"
        SELECT fear_greed_index, reddit_crypto, reddit_btc, reddit_gold
        FROM sentiment_signals
        ORDER BY timestamp DESC
        LIMIT 1
        "#
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    if let Some(mrate) = latest_mrate {
        if let Some(fg_index) = mrate.fear_greed_index.map(|f| f as i32) {
            if fg_index < 20 && direction == "long" {
                recommendations.push(AdvisoryMessage {
                    severity: "low".to_string(),
                    icon: "😱".to_string(),
                    message: format!(
                        "Extreme Fear (index: {}) - Contrarian opportunity for LONGs",
                        fg_index
                    ),
                    source: "sentiment".to_string(),
                });
                confidence_score *= 1.2;
            } else if fg_index > 80 && direction == "short" {
                recommendations.push(AdvisoryMessage {
                    severity: "low".to_string(),
                    icon: "🤑".to_string(),
                    message: format!(
                        "Extreme Greed (index: {}) - Contrarian opportunity for SHORTs",
                        fg_index
                    ),
                    source: "sentiment".to_string(),
                });
                confidence_score *= 1.2;
            } else if (fg_index < 20 && direction == "short") || (fg_index > 80 && direction == "long") {
                warnings.push(AdvisoryMessage {
                    severity: "medium".to_string(),
                    icon: "⚡".to_string(),
                    message: format!(
                        "Trading against sentiment (Fear/Greed: {}) - Higher risk",
                        fg_index
                    ),
                    source: "sentiment".to_string(),
                });
                confidence_score *= 0.85;
            }
        }
    }
    
    // 4. ENSEMBLE PATTERN CHECK (if we have recent data)
    // Note: This is simplified - in full integration, calculate win_rate from outcomes
    let recent_cluster_data = sqlx::query!(
        r#"
        SELECT cluster_id,
               COUNT(*) as trade_count,
               AVG(CASE WHEN outcome = 'win' THEN 1.0 ELSE 0.0 END) as calc_win_rate
        FROM cross_bot_learnings
        WHERE cluster_id LIKE $1
          AND created_at > NOW() - INTERVAL '30 days'
        GROUP BY cluster_id
        LIMIT 1
        "#,
        format!("{}%", instrument)
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    if let Some(cluster) = recent_cluster_data {
        let trade_count = cluster.trade_count.unwrap_or(0) as i32;
        if trade_count >= 5 {
            let win_rate = cluster.calc_win_rate.map(|v| v.to_string().parse().unwrap_or(0.5)).unwrap_or(0.5);
            if win_rate > 0.65 {
                recommendations.push(AdvisoryMessage {
                    severity: "low".to_string(),
                    icon: "🎯".to_string(),
                    message: format!(
                        "Strong cluster performance: {:.0}% win rate in similar conditions",
                        win_rate * 100.0
                    ),
                    source: "ensemble".to_string(),
                });
                confidence_score *= 1.15;
            } else if win_rate < 0.35 && trade_count >= 10 {
                warnings.push(AdvisoryMessage {
                    severity: "high".to_string(),
                    icon: "❌".to_string(),
                    message: format!(
                        "Cluster struggling: {:.0}% win rate - Consider skipping",
                        win_rate * 100.0
                    ),
                    source: "ensemble".to_string(),
                });
                confidence_score *= 0.6;
            }
        }
    }
    
    // 5. EVENT IMPACT CHECK
    let upcoming_events = sqlx::query!(
        r#"
        SELECT event_name, impact_score
        FROM event_impacts
        WHERE impact_score >= 7
        ORDER BY impact_score DESC
        LIMIT 3
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    
    if !upcoming_events.is_empty() {
        let high_impact_names: Vec<String> = upcoming_events
            .iter()
            .map(|e| e.event_name.clone())
            .collect();
        
        warnings.push(AdvisoryMessage {
            severity: "medium".to_string(),
            icon: "📰".to_string(),
            message: format!(
                "High-impact events in system: {} - Expect volatility",
                high_impact_names.join(", ")
            ),
            source: "events".to_string(),
        });
    }
    
    // DETERMINE OVERALL RECOMMENDATION
    let overall_recommendation = if confidence_score < 0.6 || size_multiplier < 0.7 {
        "SKIP".to_string()
    } else if confidence_score < 0.85 || size_multiplier < 0.85 {
        "CAUTION".to_string()
    } else {
        "START".to_string()
    };
    
    // Clamp values
    confidence_score = confidence_score.clamp(0.0, 1.5);
    size_multiplier = size_multiplier.clamp(0.5, 1.5);
    
    Ok(HttpResponse::Ok().json(AdvisoryResponse {
        instrument: instrument.clone(),
        direction: direction.clone(),
        overall_recommendation,
        confidence_score,
        position_size_multiplier: size_multiplier,
        warnings,
        recommendations,
        regime_analysis,
        generated_at: Utc::now().to_rfc3339(),
    }))
}

/// GET /api/ai/live-trades - Live trade analysis with AI recommendations
#[derive(Debug, Serialize)]
pub struct LiveTradesResponse {
    pub trades: Vec<LiveTradeInfo>,
    pub insights: LiveTradeInsights,
    pub mrate_feedback: MrateFeedback,
}

#[derive(Debug, Serialize)]
pub struct LiveTradeInfo {
    pub trade_id: String,
    pub bot_name: String,
    pub instrument: String,
    pub direction: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub units: f64,
    pub time_in_trade_minutes: i64,
    pub recommendation: String,
    pub recommendation_icon: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub regime_aligned: bool,
    pub current_regime: String,
    pub trend_direction: String,
    pub momentum_score: f64,
    pub reversal_probability: f64,
}

#[derive(Debug, Serialize)]
pub struct LiveTradeInsights {
    pub total_open_trades: usize,
    pub total_unrealized_pnl: f64,
    pub net_direction_bias: f64,
    pub regime_alignment_pct: f64,
    pub avg_time_in_trade_minutes: f64,
    pub trades_needing_attention: usize,
    pub dominant_instrument: Option<String>,
    pub position_concentration: f64,
}

#[derive(Debug, Serialize)]
pub struct MrateFeedback {
    pub has_open_positions: bool,
    pub net_long_bias: f64,
    pub regime_alignment: f64,
    pub avg_profit_pct: f64,
    pub position_stress_score: f64,
    pub suggested_regime_adjustment: Option<String>,
}

#[get("/ai/live-trades")]
pub async fn get_live_trades(
    pool: web::Data<PgPool>,
    config: web::Data<crate::config::Config>,
    mrate_state: web::Data<crate::mrate::MrateState>,
) -> Result<HttpResponse> {
    // Get current MRATE state
    let mrate_guard = mrate_state.read().await;
    let mrate = match mrate_guard.as_ref() {
        Some(m) => m.clone(),
        None => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "error": "MRATE not initialized",
                "trades": [],
                "insights": {
                    "total_open_trades": 0,
                    "total_unrealized_pnl": 0.0
                }
            })));
        }
    };
    drop(mrate_guard);
    
    // Create OANDA client
    let oanda = match (&config.oanda_api_token, &config.oanda_account_id) {
        (Some(token), Some(account_id)) => {
            match OandaClient::new(token, account_id, config.oanda_practice) {
                Ok(client) => client,
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": format!("Failed to create OANDA client: {}", e)
                    })));
                }
            }
        }
        _ => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "error": "OANDA not configured",
                "trades": [],
                "insights": {
                    "total_open_trades": 0,
                    "total_unrealized_pnl": 0.0
                }
            })));
        }
    };
    
    // Analyze open trades
    let (analyses, insights) = match TradeAnalyzer::analyze_open_trades(
        pool.get_ref(),
        &oanda,
        &mrate,
    ).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Trade analysis failed: {}", e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Trade analysis failed: {}", e)
            })));
        }
    };
    
    // Convert to API response format
    let trades: Vec<LiveTradeInfo> = analyses.iter().map(|a| LiveTradeInfo {
        trade_id: a.trade_id.clone(),
        bot_name: a.bot_name.clone(),
        instrument: a.instrument.clone(),
        direction: a.direction.clone(),
        entry_price: a.entry_price,
        current_price: a.current_price,
        unrealized_pnl: a.unrealized_pnl,
        unrealized_pnl_pct: a.unrealized_pnl_pct,
        units: a.units,
        time_in_trade_minutes: a.time_in_trade_minutes,
        recommendation: a.recommendation.as_str().to_string(),
        recommendation_icon: a.recommendation.icon().to_string(),
        confidence: a.confidence,
        reasons: a.reasons.clone(),
        regime_aligned: a.regime_aligned,
        current_regime: a.current_regime.clone(),
        trend_direction: a.trend_direction.clone(),
        momentum_score: a.momentum_score,
        reversal_probability: a.reversal_probability,
    }).collect();
    
    let api_insights = LiveTradeInsights {
        total_open_trades: insights.total_open_trades,
        total_unrealized_pnl: insights.total_unrealized_pnl,
        net_direction_bias: insights.net_direction_bias,
        regime_alignment_pct: insights.regime_alignment_pct,
        avg_time_in_trade_minutes: insights.avg_time_in_trade_minutes,
        trades_needing_attention: insights.trades_needing_attention,
        dominant_instrument: insights.dominant_instrument.clone(),
        position_concentration: insights.position_concentration,
    };
    
    // Generate MRATE feedback based on position analysis
    let avg_profit = if !analyses.is_empty() {
        analyses.iter().map(|a| a.unrealized_pnl_pct).sum::<f64>() / analyses.len() as f64
    } else {
        0.0
    };
    
    // Position stress: high if many trades need attention or positions are concentrated
    let stress_score = if insights.total_open_trades > 0 {
        let attention_ratio = insights.trades_needing_attention as f64 / insights.total_open_trades as f64;
        let concentration_stress = insights.position_concentration;
        let loss_stress = if avg_profit < -1.0 { 0.3 } else { 0.0 };
        ((attention_ratio * 0.4 + concentration_stress * 0.3 + loss_stress) * 100.0).min(100.0)
    } else {
        0.0
    };
    
    // Suggest regime adjustment if positions are strongly misaligned
    let suggested_adjustment = if insights.regime_alignment_pct < 30.0 && insights.total_open_trades >= 2 {
        Some("Consider tightening stops - positions misaligned with current regime".to_string())
    } else if stress_score > 70.0 {
        Some("High position stress - reduce new entries until positions resolve".to_string())
    } else {
        None
    };
    
    let mrate_feedback = MrateFeedback {
        has_open_positions: insights.total_open_trades > 0,
        net_long_bias: insights.net_direction_bias,
        regime_alignment: insights.regime_alignment_pct,
        avg_profit_pct: avg_profit,
        position_stress_score: stress_score,
        suggested_regime_adjustment: suggested_adjustment,
    };
    
    Ok(HttpResponse::Ok().json(LiveTradesResponse {
        trades,
        insights: api_insights,
        mrate_feedback,
    }))
}

/// GET /api/ai/recommendations - AI Brain operational recommendations
#[get("/ai/recommendations")]
pub async fn get_ai_recommendations(
    pool: web::Data<PgPool>,
    mrate_state: web::Data<crate::mrate::MrateState>,
) -> Result<HttpResponse> {
    use crate::mrate::orchestrator::Orchestrator;
    
    // Get current MRATE state
    let mrate_guard = mrate_state.read().await;
    let mrate = match mrate_guard.as_ref() {
        Some(m) => m.clone(),
        None => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "error": "MRATE not initialized",
                "recommendations": [],
                "healthy": false
            })));
        }
    };
    drop(mrate_guard);
    
    let orchestrator = Orchestrator::new(pool.get_ref().clone());
    
    match orchestrator.analyze_for_ai_brain(&mrate).await {
        Ok(analysis) => Ok(HttpResponse::Ok().json(analysis)),
        Err(e) => {
            tracing::error!("AI Brain analysis failed: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("Analysis failed: {}", e)
            })))
        }
    }
}

/// GET /api/ai/brain-state - Complete AI Brain state with ALL platform data
/// This is the master endpoint that aggregates everything for AI analysis
#[derive(Debug, Serialize)]
pub struct AIBrainStateResponse {
    pub timestamp: String,
    
    // Current Market Conditions (from MRATE)
    pub market_state: MarketStateSection,
    
    // All Data Feeds Summary
    pub data_feeds: DataFeedsSection,
    
    // Live Trading State
    pub trading_state: TradingStateSection,
    
    // Recent Activity Log
    pub activity: ActivitySection,
    
    // Learning & Performance
    pub learning: LearningSection,
    
    // AI Synthesis - what the brain "thinks"
    pub synthesis: AISynthesis,
}

#[derive(Debug, Serialize)]
pub struct MarketStateSection {
    pub regime: String,
    pub regime_confidence: f64,
    pub liquidity_score: f64,
    pub risk_score: f64,
    pub uncertainty_score: f64,
    pub risk_multiplier: f64,
    pub favored_instrument: String,
    pub liquidity_momentum: f64,
    pub risk_momentum: f64,
    pub strategy_weights: serde_json::Value,
    pub minutes_in_regime: i64,
}

#[derive(Debug, Serialize)]
pub struct DataFeedsSection {
    // Fear & Greed
    pub fear_greed_cnn: Option<f64>,
    pub fear_greed_altme: Option<f64>,
    pub fear_greed_label: String,
    
    // Reddit Sentiment
    pub reddit_crypto: Option<f64>,
    pub reddit_btc: Option<f64>,
    pub reddit_gold: Option<f64>,
    pub reddit_consensus: String,
    
    // Market Indicators
    pub vix_level: Option<f64>,
    pub dxy_level: Option<f64>,
    pub yield_10y: Option<f64>,
    pub yield_2y: Option<f64>,
    
    // Trends
    pub btc_trend: Option<String>,
    pub gold_trend: Option<String>,
    pub sp500_trend: Option<String>,
    pub dxy_trend: Option<String>,
    
    // Economic Events
    pub high_impact_event_soon: bool,
    pub hours_to_event: Option<f64>,
    pub next_event_weight: f64,
    
    // Feed Health
    pub feeds_healthy: i32,
    pub feeds_total: i32,
    pub last_feed_update: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TradingStateSection {
    pub open_trades_count: i32,
    pub total_unrealized_pnl: f64,
    pub net_exposure_bias: f64,
    pub trades_needing_attention: i32,
    pub regime_aligned_pct: f64,
    pub dominant_instrument: Option<String>,
    pub active_bots_count: i32,
    pub paused_bots_count: i32,
    pub account_equity: Option<f64>,
    pub margin_used_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ActivitySection {
    pub recent_signals: i32,
    pub recent_orders: i32,
    pub recent_errors: i32,
    pub last_trade_time: Option<String>,
    pub last_signal_time: Option<String>,
    pub signals_last_hour: i32,
    pub trades_last_24h: i32,
    pub win_rate_last_24h: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LearningSection {
    pub total_patterns_learned: i32,
    pub active_clusters: i32,
    pub best_cluster_win_rate: Option<f64>,
    pub worst_cluster_win_rate: Option<f64>,
    pub event_impacts_tracked: i32,
    pub correlation_data_age_hours: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct AISynthesis {
    pub overall_confidence: f64,
    pub market_assessment: String,
    pub position_assessment: String,
    pub recommended_action: String,
    pub key_risks: Vec<String>,
    pub opportunities: Vec<String>,
    pub alerts: Vec<AIAlert>,
}

#[derive(Debug, Serialize)]
pub struct AIAlert {
    pub severity: String,
    pub category: String,
    pub message: String,
    pub suggested_action: Option<String>,
}

#[get("/ai/brain-state")]
pub async fn get_brain_state(
    pool: web::Data<PgPool>,
    config: web::Data<crate::config::Config>,
    mrate_state: web::Data<crate::mrate::MrateState>,
) -> Result<HttpResponse> {
    use crate::bot::activity::{get_all_active_activities, ActivityType};
    
    // Get MRATE state
    let mrate_guard = mrate_state.read().await;
    let mrate = mrate_guard.as_ref().cloned();
    drop(mrate_guard);
    
    // Build market state section
    let market_state = if let Some(ref m) = mrate {
        MarketStateSection {
            regime: m.regime.as_str().to_string(),
            regime_confidence: 1.0 - (m.uncertainty_score / 100.0),
            liquidity_score: m.liquidity_score,
            risk_score: m.risk_score,
            uncertainty_score: m.uncertainty_score,
            risk_multiplier: m.risk_multiplier,
            favored_instrument: m.favored_instrument.as_str().to_string(),
            liquidity_momentum: m.liquidity_momentum,
            risk_momentum: m.risk_momentum,
            strategy_weights: serde_json::json!({
                "trend": m.strategy_weights.trend,
                "breakout": m.strategy_weights.breakout,
                "mean_reversion": m.strategy_weights.mean_reversion,
                "liquidity_sweep": m.strategy_weights.liquidity_sweep,
            }),
            minutes_in_regime: 0, // Would need regime state tracking
        }
    } else {
        MarketStateSection {
            regime: "UNKNOWN".to_string(),
            regime_confidence: 0.0,
            liquidity_score: 50.0,
            risk_score: 50.0,
            uncertainty_score: 100.0,
            risk_multiplier: 0.5,
            favored_instrument: "NEITHER".to_string(),
            liquidity_momentum: 0.0,
            risk_momentum: 0.0,
            strategy_weights: serde_json::json!({}),
            minutes_in_regime: 0,
        }
    };
    
    // Build data feeds section
    let inputs = mrate.as_ref().and_then(|m| m.inputs.as_ref());
    let fg_idx = inputs.and_then(|i| i.fear_greed_index);
    let data_feeds = DataFeedsSection {
        fear_greed_cnn: fg_idx,
        fear_greed_altme: inputs.and_then(|i| i.alt_fear_greed_index),
        fear_greed_label: match fg_idx.unwrap_or(50.0) as i32 {
            0..=20 => "Extreme Fear".to_string(),
            21..=40 => "Fear".to_string(),
            41..=60 => "Neutral".to_string(),
            61..=80 => "Greed".to_string(),
            _ => "Extreme Greed".to_string(),
        },
        reddit_crypto: inputs.and_then(|i| i.reddit_crypto_sentiment),
        reddit_btc: inputs.and_then(|i| i.reddit_btc_sentiment),
        reddit_gold: inputs.and_then(|i| i.reddit_gold_sentiment),
        reddit_consensus: if inputs.map(|i| i.reddit_bullish_consensus).unwrap_or(false) {
            "BULLISH".to_string()
        } else if inputs.map(|i| i.reddit_bearish_consensus).unwrap_or(false) {
            "BEARISH".to_string()
        } else {
            "MIXED".to_string()
        },
        vix_level: inputs.and_then(|i| i.vix_level),
        dxy_level: inputs.and_then(|i| i.dxy_level),
        yield_10y: inputs.and_then(|i| i.yield_10y),
        yield_2y: inputs.and_then(|i| i.yield_2y),
        btc_trend: inputs.and_then(|i| i.btc_trend.as_ref()).map(|t| format!("{:?}", t)),
        gold_trend: inputs.and_then(|i| i.sp500_trend.as_ref()).map(|t| format!("{:?}", t)), // Using SP500 as proxy
        sp500_trend: inputs.and_then(|i| i.sp500_trend.as_ref()).map(|t| format!("{:?}", t)),
        dxy_trend: inputs.and_then(|i| i.dxy_trend.as_ref()).map(|t| format!("{:?}", t)),
        high_impact_event_soon: inputs.map(|i| i.high_impact_event_within_24h).unwrap_or(false),
        hours_to_event: inputs.and_then(|i| i.hours_to_next_high_impact),
        next_event_weight: inputs.map(|i| i.next_event_weight).unwrap_or(1.0),
        feeds_healthy: count_healthy_feeds(inputs),
        feeds_total: 20,
        last_feed_update: mrate.as_ref().map(|m| m.timestamp.to_rfc3339()),
    };
    
    // Query trading state from database
    let active_bots = sqlx::query_scalar!("SELECT COUNT(*) FROM bots WHERE is_active = true")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0) as i32;
    
    let paused_bots = sqlx::query_scalar!("SELECT COUNT(*) FROM bots WHERE is_active = false")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0) as i32;
    
    let trading_state = TradingStateSection {
        open_trades_count: 0, // Would need OANDA query
        total_unrealized_pnl: 0.0,
        net_exposure_bias: 0.0,
        trades_needing_attention: 0,
        regime_aligned_pct: 0.0,
        dominant_instrument: None,
        active_bots_count: active_bots,
        paused_bots_count: paused_bots,
        account_equity: None,
        margin_used_pct: None,
    };
    
    // Query activity stats
    let signals_last_hour = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_activities WHERE activity_type = 'signal' AND created_at > NOW() - INTERVAL '1 hour'"
    )
    .fetch_one(pool.get_ref())
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0) as i32;
    
    let trades_24h = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_activities WHERE activity_type IN ('order_filled', 'order_closed') AND created_at > NOW() - INTERVAL '24 hours'"
    )
    .fetch_one(pool.get_ref())
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0) as i32;
    
    let recent_errors = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM bot_activities WHERE activity_type = 'error' AND created_at > NOW() - INTERVAL '1 hour'"
    )
    .fetch_one(pool.get_ref())
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0) as i32;
    
    let last_signal = sqlx::query_scalar!(
        "SELECT MAX(created_at)::text FROM bot_activities WHERE activity_type = 'signal'"
    )
    .fetch_one(pool.get_ref())
    .await
    .ok()
    .flatten();
    
    let activity = ActivitySection {
        recent_signals: signals_last_hour,
        recent_orders: trades_24h,
        recent_errors,
        last_trade_time: None,
        last_signal_time: last_signal,
        signals_last_hour,
        trades_last_24h: trades_24h,
        win_rate_last_24h: None,
    };
    
    // Query learning stats
    let total_patterns = sqlx::query_scalar!("SELECT COUNT(*) FROM cross_bot_learnings")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0) as i32;
    
    let event_impacts = sqlx::query_scalar!("SELECT COUNT(*) FROM event_impacts")
        .fetch_one(pool.get_ref())
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0) as i32;
    
    let learning = LearningSection {
        total_patterns_learned: total_patterns,
        active_clusters: 0,
        best_cluster_win_rate: None,
        worst_cluster_win_rate: None,
        event_impacts_tracked: event_impacts,
        correlation_data_age_hours: None,
    };
    
    // AI Synthesis - the "thinking" part
    let mut key_risks = Vec::new();
    let mut opportunities = Vec::new();
    let mut alerts = Vec::new();
    
    // Analyze and synthesize
    if market_state.uncertainty_score > 70.0 {
        key_risks.push("High market uncertainty - conditions unpredictable".to_string());
    }
    if market_state.risk_multiplier < 0.7 {
        key_risks.push(format!("Risk multiplier low ({:.2}) - reduced position sizes", market_state.risk_multiplier));
    }
    if data_feeds.high_impact_event_soon {
        key_risks.push(format!("High-impact event in {:.1} hours", data_feeds.hours_to_event.unwrap_or(24.0)));
        alerts.push(AIAlert {
            severity: "warning".to_string(),
            category: "events".to_string(),
            message: "High-impact economic event approaching".to_string(),
            suggested_action: Some("Consider reducing exposure or tightening stops".to_string()),
        });
    }
    if recent_errors > 3 {
        alerts.push(AIAlert {
            severity: "error".to_string(),
            category: "system".to_string(),
            message: format!("{} errors in the last hour", recent_errors),
            suggested_action: Some("Check bot logs for issues".to_string()),
        });
    }
    
    // Opportunities
    if let Some(fg) = data_feeds.fear_greed_cnn {
        if fg < 25.0 {
            opportunities.push("Extreme fear - potential contrarian long opportunities".to_string());
        } else if fg > 75.0 {
            opportunities.push("Extreme greed - potential contrarian short opportunities".to_string());
        }
    }
    if market_state.regime == "TREND" && market_state.liquidity_momentum > 0.3 {
        opportunities.push("Strong trend with improving liquidity - trend-following favorable".to_string());
    }
    
    let overall_confidence = (100.0 - market_state.uncertainty_score) / 100.0 * market_state.regime_confidence;
    
    let market_assessment = match market_state.regime.as_str() {
        "TREND" => "Markets trending - directional strategies favored",
        "CHOPPY" => "Choppy conditions - mean reversion strategies favored",
        "GOLD_SUPER_BULL" => "Gold super bull - aggressive gold longs favored",
        "BTC_SUPER_BULL" => "BTC super bull - aggressive BTC longs favored",
        "PANIC" => "Panic conditions - defensive positioning, short opportunities",
        _ => "Unknown regime",
    }.to_string();
    
    let position_assessment = if trading_state.open_trades_count == 0 {
        "No open positions - ready to deploy capital".to_string()
    } else if trading_state.trades_needing_attention > 0 {
        format!("{} positions need attention", trading_state.trades_needing_attention)
    } else {
        format!("{} positions active, P&L: ${:.2}", trading_state.open_trades_count, trading_state.total_unrealized_pnl)
    };
    
    let recommended_action = if overall_confidence < 0.3 {
        "REDUCE_EXPOSURE"
    } else if overall_confidence < 0.5 {
        "CAUTIOUS_TRADING"
    } else if overall_confidence < 0.7 {
        "NORMAL_OPERATIONS"
    } else {
        "FAVORABLE_CONDITIONS"
    }.to_string();
    
    let synthesis = AISynthesis {
        overall_confidence,
        market_assessment,
        position_assessment,
        recommended_action,
        key_risks,
        opportunities,
        alerts,
    };
    
    Ok(HttpResponse::Ok().json(AIBrainStateResponse {
        timestamp: Utc::now().to_rfc3339(),
        market_state,
        data_feeds,
        trading_state,
        activity,
        learning,
        synthesis,
    }))
}

/// GET /api/ai/mrate-advisor - MRATE Advisor with contradictions and suggestions
#[derive(Debug, Serialize)]
pub struct MrateAdvisorResponse {
    pub timestamp: String,
    pub health_score: f64,
    pub summary: String,
    pub contradictions: Vec<ContradictionInfo>,
    pub suggestions: Vec<SuggestionInfo>,
    pub blocked_opportunities: Vec<BlockedOpportunityInfo>,
    pub regime_performance: Vec<RegimePerformanceInfo>,
}

#[derive(Debug, Serialize)]
pub struct ContradictionInfo {
    pub contradiction_type: String,
    pub instrument: String,
    pub severity: String,
    pub icon: String,
    pub description: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SuggestionInfo {
    pub id: String,
    pub priority: String,
    pub category: String,
    pub category_icon: String,
    pub title: String,
    pub description: String,
    pub current_value: Option<String>,
    pub suggested_value: Option<String>,
    pub expected_impact: String,
    pub confidence: f64,
    pub supporting_data: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BlockedOpportunityInfo {
    pub instrument: String,
    pub strategy_category: String,
    pub block_reason: String,
    pub potential_direction: String,
    pub adx_value: f64,
    pub price_regime: String,
}

#[derive(Debug, Serialize)]
pub struct RegimePerformanceInfo {
    pub regime: String,
    pub total_trades: i32,
    pub win_rate: f64,
    pub total_pnl: f64,
}

#[get("/ai/mrate-advisor")]
pub async fn get_mrate_advisor(
    pool: web::Data<PgPool>,
    config: web::Data<crate::config::Config>,
    mrate_state: web::Data<crate::mrate::MrateState>,
) -> Result<HttpResponse> {
    // Get current MRATE state
    let mrate_guard = mrate_state.read().await;
    let mrate = match mrate_guard.as_ref() {
        Some(m) => m.clone(),
        None => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "error": "MRATE not initialized",
                "health_score": 0,
                "summary": "Waiting for MRATE initialization",
                "contradictions": [],
                "suggestions": [],
                "blocked_opportunities": [],
                "regime_performance": []
            })));
        }
    };
    drop(mrate_guard);
    
    // Get open positions from OANDA if configured
    let open_positions = match (&config.oanda_api_token, &config.oanda_account_id) {
        (Some(token), Some(account_id)) => {
            match OandaClient::new(token, account_id, config.oanda_practice) {
                Ok(oanda) => {
                    match oanda.get_open_trades().await {
                        Ok(trades) => {
                            trades.iter().filter_map(|trade| {
                                let units: f64 = trade.current_units.parse().ok()?;
                                let entry_price: f64 = trade.price.parse().ok()?;
                                let unrealized_pnl: f64 = trade.unrealized_pl
                                    .as_ref()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0.0);
                                let pnl_pct = if entry_price > 0.0 {
                                    (unrealized_pnl / (units.abs() * entry_price)) * 100.0
                                } else {
                                    0.0
                                };
                                
                                Some(PositionInfo {
                                    instrument: trade.instrument.clone(),
                                    direction: if units > 0.0 { "LONG".to_string() } else { "SHORT".to_string() },
                                    unrealized_pnl,
                                    unrealized_pnl_pct: pnl_pct,
                                    entry_price,
                                    current_price: entry_price, // Would need price lookup for accurate value
                                })
                            }).collect()
                        },
                        Err(_) => vec![],
                    }
                },
                Err(_) => vec![],
            }
        },
        _ => vec![],
    };
    
    // Run the advisor analysis
    let advisor = MrateAdvisor::new(pool.get_ref().clone());
    let output = advisor.analyze(&mrate, &open_positions).await;
    
    // Convert to API response format
    let contradictions: Vec<ContradictionInfo> = output.contradictions.iter().map(|c| ContradictionInfo {
        contradiction_type: c.contradiction_type.as_str().to_string(),
        instrument: c.instrument.clone(),
        severity: c.severity.clone(),
        icon: c.icon.clone(),
        description: c.description.clone(),
        evidence: c.evidence.clone(),
    }).collect();
    
    let suggestions: Vec<SuggestionInfo> = output.suggestions.iter().map(|s| SuggestionInfo {
        id: s.id.clone(),
        priority: s.priority.as_str().to_string(),
        category: s.category.as_str().to_string(),
        category_icon: s.category.icon().to_string(),
        title: s.title.clone(),
        description: s.description.clone(),
        current_value: s.current_value.clone(),
        suggested_value: s.suggested_value.clone(),
        expected_impact: s.expected_impact.clone(),
        confidence: s.confidence,
        supporting_data: s.supporting_data.clone(),
    }).collect();
    
    let blocked_opportunities: Vec<BlockedOpportunityInfo> = output.blocked_opportunities.iter().map(|b| BlockedOpportunityInfo {
        instrument: b.instrument.clone(),
        strategy_category: b.strategy_category.clone(),
        block_reason: b.block_reason.clone(),
        potential_direction: b.potential_direction.clone(),
        adx_value: b.adx_value,
        price_regime: b.price_regime.clone(),
    }).collect();
    
    let regime_performance: Vec<RegimePerformanceInfo> = output.performance_by_regime.iter().map(|r| RegimePerformanceInfo {
        regime: r.regime.clone(),
        total_trades: r.total_trades,
        win_rate: r.win_rate,
        total_pnl: r.total_pnl,
    }).collect();
    
    Ok(HttpResponse::Ok().json(MrateAdvisorResponse {
        timestamp: output.timestamp.to_rfc3339(),
        health_score: output.health_score,
        summary: output.summary,
        contradictions,
        suggestions,
        blocked_opportunities,
        regime_performance,
    }))
}

/// Helper to count healthy data feeds
fn count_healthy_feeds(inputs: Option<&crate::mrate::models::MrateInputs>) -> i32 {
    let Some(i) = inputs else { return 0 };
    let mut count = 0;
    if i.fear_greed_index.is_some() { count += 1; }
    if i.vix_level.is_some() { count += 1; }
    if i.dxy_level.is_some() { count += 1; }
    if i.yield_10y.is_some() { count += 1; }
    if i.yield_2y.is_some() { count += 1; }
    if i.btc_trend.is_some() { count += 1; }
    if i.sp500_trend.is_some() { count += 1; }
    if i.dxy_trend.is_some() { count += 1; }
    if i.reddit_crypto_sentiment.is_some() { count += 1; }
    if i.reddit_btc_sentiment.is_some() { count += 1; }
    if i.reddit_gold_sentiment.is_some() { count += 1; }
    if i.alt_fear_greed_index.is_some() { count += 1; }
    if i.btc_atr_percentile.is_some() { count += 1; }
    if i.atr_percentile.is_some() { count += 1; }
    if i.stablecoin_dominance.is_some() { count += 1; }
    if i.btc_exchange_netflow.is_some() { count += 1; }
    if i.gold_etf_flow_weekly.is_some() { count += 1; }
    if i.oil_trend.is_some() { count += 1; }
    if i.natgas_trend.is_some() { count += 1; }
    if i.eur_usd_trend.is_some() { count += 1; }
    count
}

/// Configure AI Brain routes
pub fn configure_ai_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_ensemble_stats)
        .service(get_regime_prediction)
        .service(get_evolution_stats)
        .service(get_correlation_matrix)
        .service(get_event_impacts)
        .service(get_sentiment_signals)
        .service(get_trailing_stops)
        .service(get_trading_advisory)
        .service(get_live_trades)
        .service(get_brain_state)
        .service(get_ai_recommendations)
        .service(get_mrate_advisor);
}
