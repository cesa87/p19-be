//! Risk Engine API endpoints
//!
//! Provides real-time risk metrics and status for the dashboard

use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::risk::PortfolioRiskEngine;
use crate::error::AppError;

/// Risk status response for the dashboard
#[derive(Debug, Serialize)]
pub struct RiskStatusResponse {
    pub account_equity: f64,
    pub peak_equity: f64,
    pub current_drawdown_pct: f64,
    pub daily_pnl: f64,
    pub daily_loss_pct: f64,
    pub weekly_pnl: f64,
    pub weekly_loss_pct: f64,
    pub consecutive_losses: u32,
    pub kill_switch_active: bool,
    pub total_exposure: f64,
    pub exposure_pct_of_equity: f64,
    pub current_decision: String,
    pub warnings: Vec<String>,
    pub limits: RiskLimitsResponse,
    pub metrics: TradeMetrics,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct TradeMetrics {
    pub total_trades: u32,
    pub win_rate: f64,
    pub avg_risk_per_trade_pct: f64,
    pub avg_rr_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct RiskLimitsResponse {
    pub max_daily_loss_pct: f64,
    pub max_weekly_loss_pct: f64,
    pub max_drawdown_pct: f64,
    pub max_consecutive_losses: u32,
    pub max_total_exposure_multiplier: f64,
    pub max_per_asset_multiplier: f64,
    pub max_per_strategy_multiplier: f64,
}

/// Exposure breakdown response
#[derive(Debug, Serialize)]
pub struct ExposureResponse {
    pub total_exposure: f64,
    pub by_asset: Vec<AssetExposure>,
    pub by_strategy: Vec<StrategyExposure>,
    pub open_positions: Vec<PositionResponse>,
}

#[derive(Debug, Serialize)]
pub struct AssetExposure {
    pub asset: String,
    pub exposure: f64,
    pub pct_of_equity: f64,
    pub limit: f64,
    pub pct_of_limit: f64,
}

#[derive(Debug, Serialize)]
pub struct StrategyExposure {
    pub strategy_id: String,
    pub exposure: f64,
    pub pct_of_equity: f64,
    pub limit: f64,
    pub pct_of_limit: f64,
}

#[derive(Debug, Serialize)]
pub struct PositionResponse {
    pub position_id: String,
    pub bot_id: String,
    pub strategy_id: String,
    pub instrument: String,
    pub direction: String,
    pub entry_price: f64,
    pub lot_size: f64,
    pub notional_value: f64,
    pub opened_at: String,
}

/// GET /api/risk/status - Get current risk engine status
pub async fn get_risk_status(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    let status = risk_engine.get_risk_status().await;
    let limits = risk_engine.get_limits().await;
    
    let decision_str = match status.current_decision {
        crate::risk::RiskDecision::AllowTrading => "ALLOW_TRADING".to_string(),
        crate::risk::RiskDecision::ReducePositionSize(mult) => {
            format!("REDUCE_SIZE_{}", (mult * 100.0) as u32)
        },
        crate::risk::RiskDecision::BlockNewTrades => "BLOCK_TRADES".to_string(),
        crate::risk::RiskDecision::EmergencyShutdown => "EMERGENCY_SHUTDOWN".to_string(),
    };
    
    // Calculate trade metrics
    let total_trades = status.account_state.total_trades;
    let winning_trades = status.account_state.winning_trades;
    let win_rate = if total_trades > 0 {
        (winning_trades as f64 / total_trades as f64) * 100.0
    } else {
        0.0
    };
    
    let response = RiskStatusResponse {
        account_equity: status.account_state.equity,
        peak_equity: status.account_state.peak_equity,
        current_drawdown_pct: status.drawdown_pct,
        daily_pnl: status.account_state.daily_pnl,
        daily_loss_pct: status.daily_loss_pct,
        weekly_pnl: status.account_state.weekly_pnl,
        weekly_loss_pct: status.weekly_loss_pct,
        consecutive_losses: status.account_state.consecutive_losses,
        kill_switch_active: status.account_state.kill_switch_active,
        total_exposure: status.total_exposure,
        exposure_pct_of_equity: status.exposure_pct_of_equity,
        current_decision: decision_str,
        warnings: status.warnings,
        limits: RiskLimitsResponse {
            max_daily_loss_pct: limits.max_daily_loss_pct,
            max_weekly_loss_pct: limits.max_weekly_loss_pct,
            max_drawdown_pct: limits.max_drawdown_pct,
            max_consecutive_losses: limits.max_consecutive_losses,
            max_total_exposure_multiplier: limits.max_total_exposure_multiplier,
            max_per_asset_multiplier: limits.max_per_asset_multiplier,
            max_per_strategy_multiplier: limits.max_per_strategy_multiplier,
        },
        metrics: TradeMetrics {
            total_trades,
            win_rate,
            avg_risk_per_trade_pct: 0.5,  // Default - could be configurable
            avg_rr_ratio: 2.0,  // Default - could track actual from trades
        },
        timestamp: status.timestamp.to_rfc3339(),
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// GET /api/risk/exposure - Get exposure breakdown
pub async fn get_exposure_breakdown(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    let tracker = risk_engine.position_tracker();
    let tracker_lock = tracker.read().await;
    let account_state = risk_engine.account_state();
    let account_lock = account_state.read().await;
    
    let equity = account_lock.equity;
    let total_exposure = tracker_lock.total_exposure();
    
    // Get exposure breakdown by asset
    let asset_breakdown = tracker_lock.exposure_breakdown();
    let by_asset: Vec<AssetExposure> = asset_breakdown.into_iter()
        .map(|(asset, exposure)| {
            let limit = equity * 2.0; // max_per_asset_multiplier
            AssetExposure {
                asset,
                exposure,
                pct_of_equity: if equity > 0.0 { (exposure / equity) * 100.0 } else { 0.0 },
                limit,
                pct_of_limit: if limit > 0.0 { (exposure / limit) * 100.0 } else { 0.0 },
            }
        })
        .collect();
    
    // Get all open positions
    let positions = tracker_lock.get_all_positions();
    let open_positions: Vec<PositionResponse> = positions.into_iter()
        .map(|p| PositionResponse {
            position_id: p.position_id.to_string(),
            bot_id: p.bot_id.to_string(),
            strategy_id: p.strategy_id.to_string(),
            instrument: p.instrument,
            direction: p.direction,
            entry_price: p.entry_price,
            lot_size: p.lot_size,
            notional_value: p.notional_value,
            opened_at: p.opened_at.to_rfc3339(),
        })
        .collect();
    
    // Group positions by strategy for strategy exposure
    use std::collections::HashMap;
    let mut strategy_map: HashMap<uuid::Uuid, f64> = HashMap::new();
    for pos in tracker_lock.get_all_positions() {
        *strategy_map.entry(pos.strategy_id).or_insert(0.0) += pos.notional_value;
    }
    
    let by_strategy: Vec<StrategyExposure> = strategy_map.into_iter()
        .map(|(strategy_id, exposure)| {
            let limit = equity * 1.5; // max_per_strategy_multiplier
            StrategyExposure {
                strategy_id: strategy_id.to_string(),
                exposure,
                pct_of_equity: if equity > 0.0 { (exposure / equity) * 100.0 } else { 0.0 },
                limit,
                pct_of_limit: if limit > 0.0 { (exposure / limit) * 100.0 } else { 0.0 },
            }
        })
        .collect();
    
    let response = ExposureResponse {
        total_exposure,
        by_asset,
        by_strategy,
        open_positions,
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// Correlation data for dashboard
#[derive(Debug, Serialize)]
pub struct CorrelationResponse {
    pub matrix: Vec<StrategyCorrelation>,
    pub threshold: f64,
    pub lookback_trades: usize,
}

#[derive(Debug, Serialize)]
pub struct StrategyCorrelation {
    pub strategy_a_id: String,
    pub strategy_a_name: String,
    pub strategy_b_id: String,
    pub strategy_b_name: String,
    pub correlation: f64,
    pub is_correlated: bool,
}

/// GET /api/risk/correlation - Get strategy correlation matrix
pub async fn get_correlation_matrix(
    pool: web::Data<sqlx::PgPool>,
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    // Get all strategies that have trade history
    let strategies: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        r#"
        SELECT DISTINCT s.id, s.name
        FROM strategies s
        JOIN trade_context tc ON tc.strategy_id = s.id
        WHERE tc.pnl IS NOT NULL
          AND tc.closed_at IS NOT NULL
        ORDER BY s.name
        LIMIT 20
        "#
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| AppError::InternalError(format!("Failed to fetch strategies: {}", e)))?;
    
    let mut matrix = Vec::new();
    let threshold = 0.7;
    
    // Calculate pairwise correlations
    for i in 0..strategies.len() {
        for j in (i + 1)..strategies.len() {
            let (id_a, name_a) = &strategies[i];
            let (id_b, name_b) = &strategies[j];
            
            // Calculate actual correlation using risk engine
            let correlation = risk_engine
                .calculate_correlation(pool.get_ref(), *id_a, *id_b)
                .await
                .unwrap_or(0.0);
            
            matrix.push(StrategyCorrelation {
                strategy_a_id: id_a.to_string(),
                strategy_a_name: name_a.clone(),
                strategy_b_id: id_b.to_string(),
                strategy_b_name: name_b.clone(),
                correlation,
                is_correlated: correlation.abs() > threshold,
            });
        }
    }
    
    let response = CorrelationResponse {
        matrix,
        threshold,
        lookback_trades: 30,
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// Request to update risk limits
#[derive(Debug, Deserialize)]
pub struct UpdateLimitsRequest {
    pub max_daily_loss_pct: Option<f64>,
    pub max_weekly_loss_pct: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub max_consecutive_losses: Option<u32>,
    pub max_total_exposure_multiplier: Option<f64>,
    pub max_per_asset_multiplier: Option<f64>,
    pub max_per_strategy_multiplier: Option<f64>,
}

/// POST /api/risk/limits - Update risk limits
pub async fn update_risk_limits(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
    body: web::Json<UpdateLimitsRequest>,
) -> Result<HttpResponse, AppError> {
    // Validate inputs
    if let Some(daily) = body.max_daily_loss_pct {
        if daily < 0.5 || daily > 20.0 {
            return Err(AppError::BadRequest("max_daily_loss_pct must be between 0.5% and 20%".into()));
        }
    }
    if let Some(weekly) = body.max_weekly_loss_pct {
        if weekly < 1.0 || weekly > 30.0 {
            return Err(AppError::BadRequest("max_weekly_loss_pct must be between 1% and 30%".into()));
        }
    }
    if let Some(dd) = body.max_drawdown_pct {
        if dd < 2.0 || dd > 50.0 {
            return Err(AppError::BadRequest("max_drawdown_pct must be between 2% and 50%".into()));
        }
    }
    if let Some(consec) = body.max_consecutive_losses {
        if consec < 3 || consec > 20 {
            return Err(AppError::BadRequest("max_consecutive_losses must be between 3 and 20".into()));
        }
    }
    
    // Update the limits
    risk_engine.update_limits(
        body.max_daily_loss_pct,
        body.max_weekly_loss_pct,
        body.max_drawdown_pct,
        body.max_consecutive_losses,
        body.max_total_exposure_multiplier,
        body.max_per_asset_multiplier,
        body.max_per_strategy_multiplier,
    ).await;
    
    tracing::info!(
        "Risk limits updated: daily={:?}%, weekly={:?}%, drawdown={:?}%, consec_losses={:?}",
        body.max_daily_loss_pct, body.max_weekly_loss_pct, body.max_drawdown_pct, body.max_consecutive_losses
    );
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Risk limits updated"
    })))
}

/// POST /api/risk/reset-kill-switch - Reset the emergency kill switch
pub async fn reset_kill_switch(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    risk_engine.reset_kill_switch().await;
    
    tracing::warn!("Kill switch manually reset via API");
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Kill switch has been reset. Trading can resume.",
        "warning": "Make sure you understand why the kill switch was triggered before continuing to trade."
    })))
}

/// POST /api/risk/reset-daily - Reset daily P&L stats
pub async fn reset_daily_stats(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    let account_state = risk_engine.account_state();
    let mut account = account_state.write().await;
    account.reset_daily();
    
    tracing::info!("Daily stats manually reset via API");
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Daily stats have been reset"
    })))
}

/// POST /api/risk/update-equity - Manually update account equity
#[derive(Debug, Deserialize)]
pub struct UpdateEquityRequest {
    pub equity: f64,
}

pub async fn update_equity(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
    body: web::Json<UpdateEquityRequest>,
) -> Result<HttpResponse, AppError> {
    if body.equity <= 0.0 {
        return Err(AppError::BadRequest("Equity must be positive".into()));
    }
    
    risk_engine.update_equity(body.equity).await;
    
    tracing::info!("Account equity updated to ${:.2} via API", body.equity);
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("Equity updated to ${:.2}", body.equity)
    })))
}

/// GET /api/risk/limits - Get current risk limits
pub async fn get_risk_limits(
    risk_engine: web::Data<Arc<PortfolioRiskEngine>>,
) -> Result<HttpResponse, AppError> {
    let limits = risk_engine.get_limits().await;
    
    Ok(HttpResponse::Ok().json(RiskLimitsResponse {
        max_daily_loss_pct: limits.max_daily_loss_pct,
        max_weekly_loss_pct: limits.max_weekly_loss_pct,
        max_drawdown_pct: limits.max_drawdown_pct,
        max_consecutive_losses: limits.max_consecutive_losses,
        max_total_exposure_multiplier: limits.max_total_exposure_multiplier,
        max_per_asset_multiplier: limits.max_per_asset_multiplier,
        max_per_strategy_multiplier: limits.max_per_strategy_multiplier,
    }))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/risk")
            .route("/status", web::get().to(get_risk_status))
            .route("/exposure", web::get().to(get_exposure_breakdown))
            .route("/correlation", web::get().to(get_correlation_matrix))
            .route("/limits", web::get().to(get_risk_limits))
            .route("/limits", web::post().to(update_risk_limits))
            .route("/reset-kill-switch", web::post().to(reset_kill_switch))
            .route("/reset-daily", web::post().to(reset_daily_stats))
            .route("/update-equity", web::post().to(update_equity))
    );
}
