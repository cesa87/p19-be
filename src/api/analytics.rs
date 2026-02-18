//! Analytics API endpoints
//!
//! Provides endpoints for querying bot performance analytics

use actix_web::{web, HttpResponse, Result};
use uuid::Uuid;

use crate::analytics::performance::{
    get_bot_analytics_report, get_bot_performance_summary, get_all_bots_summary,
    get_performance_by_rsi, get_performance_by_adx, get_performance_by_regime,
    get_performance_by_session, get_performance_by_hour, get_performance_by_volatility,
    get_regime_accuracy,
};
use crate::analytics::adaptation::{
    AdaptationConfig, AdaptationEngine, get_active_adaptations, get_pending_adaptations,
    approve_adaptation, reject_adaptation, save_adaptation, ProposedAdaptation,
};
use crate::db::DbPool;
use crate::error::AppError;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/analytics")
            // Full report for a bot
            .route("/bot/{id}", web::get().to(get_bot_report))
            // Summary only
            .route("/bot/{id}/summary", web::get().to(get_bot_summary))
            // Performance by specific condition
            .route("/bot/{id}/by-rsi", web::get().to(by_rsi))
            .route("/bot/{id}/by-adx", web::get().to(by_adx))
            .route("/bot/{id}/by-regime", web::get().to(by_regime))
            .route("/bot/{id}/by-session", web::get().to(by_session))
            .route("/bot/{id}/by-hour", web::get().to(by_hour))
            .route("/bot/{id}/by-volatility", web::get().to(by_volatility))
            // Adaptations
            .route("/bot/{id}/adaptations", web::get().to(get_adaptations))
            .route("/bot/{id}/adaptations/pending", web::get().to(get_pending))
            .route("/bot/{id}/adaptations/analyze", web::post().to(analyze_adaptations))
            .route("/adaptations/{id}/approve", web::post().to(approve))
            .route("/adaptations/{id}/reject", web::post().to(reject))
            // All bots summary
            .route("/summary", web::get().to(all_bots_summary))
            // Regime accuracy validation (global)
            .route("/regime-accuracy", web::get().to(regime_accuracy))
    );
}

/// Get full analytics report for a bot
async fn get_bot_report(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let report = get_bot_analytics_report(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(report))
}

/// Get performance summary only for a bot
async fn get_bot_summary(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let summary = get_bot_performance_summary(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(summary))
}

/// Get performance breakdown by RSI
async fn by_rsi(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_rsi(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get performance breakdown by ADX
async fn by_adx(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_adx(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get performance breakdown by MRATE regime
async fn by_regime(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_regime(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get performance breakdown by trading session
async fn by_session(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_session(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get performance breakdown by hour of day
async fn by_hour(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_hour(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get performance breakdown by volatility
async fn by_volatility(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let data = get_performance_by_volatility(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

/// Get summary for all bots
async fn all_bots_summary(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    let summaries = get_all_bots_summary(pool.get_ref())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    // Convert to JSON-friendly format
    let result: Vec<serde_json::Value> = summaries.into_iter()
        .map(|(id, name, summary)| {
            serde_json::json!({
                "bot_id": id,
                "bot_name": name,
                "summary": summary
            })
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(result))
}

/// Regime accuracy validation (global, all bots)
async fn regime_accuracy(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    let data = get_regime_accuracy(pool.get_ref())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(data))
}

// ============ Adaptation Endpoints ============

/// Get active adaptations for a bot's strategy
async fn get_adaptations(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    // Get bot's strategy_id
    let bot: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT strategy_id FROM bots WHERE id = $1"
    )
    .bind(bot_id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    let strategy_id = match bot {
        Some((Some(sid),)) => sid,
        _ => return Ok(HttpResponse::Ok().json(Vec::<()>::new())),
    };
    
    let adaptations = get_active_adaptations(pool.get_ref(), strategy_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(adaptations))
}

/// Get pending adaptations for a bot
async fn get_pending(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let adaptations = get_pending_adaptations(pool.get_ref(), bot_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(adaptations))
}

/// Request body for analyze endpoint
#[derive(serde::Deserialize)]
struct AnalyzeRequest {
    strategy_id: Uuid,
    #[serde(default)]
    auto_save: bool,
}

/// Analyze and generate proposed adaptations
async fn analyze_adaptations(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<AnalyzeRequest>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    let engine = AdaptationEngine::new(AdaptationConfig::default());
    
    let proposals = engine
        .analyze_and_propose(pool.get_ref(), bot_id, body.strategy_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    // Optionally save proposals to database
    if body.auto_save {
        for proposal in &proposals {
            let _ = save_adaptation(pool.get_ref(), bot_id, body.strategy_id, proposal, false).await;
        }
    }
    
    Ok(HttpResponse::Ok().json(proposals))
}

/// Approve an adaptation
async fn approve(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let adaptation_id = path.into_inner();
    
    let adaptation = approve_adaptation(pool.get_ref(), adaptation_id, "USER")
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(adaptation))
}

/// Reject an adaptation
async fn reject(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let adaptation_id = path.into_inner();
    
    let adaptation = reject_adaptation(pool.get_ref(), adaptation_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(adaptation))
}
