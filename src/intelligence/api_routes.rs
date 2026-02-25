//! Intelligence Engine API routes
//! GET /api/intelligence/scores
//! GET /api/intelligence/feeds
//! GET /api/intelligence/events
//! GET /api/intelligence/whales
//! GET /api/intelligence/arb
//! GET /api/intelligence/arb/history
//! GET /api/intelligence/history/:instrument

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::error;

use super::aggregator::IntelligenceAggregator;
use super::scorer::IntelligenceScorer;
use super::whale_tracker::WhaleTracker;
use super::event_detector::EventDetector;
use super::arb_scanner::ArbScanner;

use std::sync::Arc;
use crate::config::Config;

#[derive(Deserialize)]
struct HistoryQuery {
    hours: Option<i64>,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ─── Scores ──────────────────────────────────────────────────────────────────

/// GET /api/intelligence/scores — latest per-instrument scores
pub async fn get_scores(pool: web::Data<PgPool>) -> impl Responder {
    let scorer = IntelligenceScorer::new(pool.get_ref().clone());
    match scorer.get_latest_scores().await {
        Ok(scores) => HttpResponse::Ok().json(scores),
        Err(e) => {
            error!("Failed to get intelligence scores: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Feeds ───────────────────────────────────────────────────────────────────

/// GET /api/intelligence/feeds — latest raw feed snapshots
pub async fn get_feeds(pool: web::Data<PgPool>, config: web::Data<Config>) -> impl Responder {
    let aggregator = IntelligenceAggregator::new(pool.get_ref().clone(), Arc::new(config.get_ref().clone()));
    match aggregator.get_latest_snapshots().await {
        Ok(snaps) => HttpResponse::Ok().json(snaps),
        Err(e) => {
            error!("Failed to get feed snapshots: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Events ──────────────────────────────────────────────────────────────────

/// GET /api/intelligence/events — recent anomaly events
pub async fn get_events(pool: web::Data<PgPool>, query: web::Query<HistoryQuery>, config: web::Data<Config>) -> impl Responder {
    let limit = query.limit.unwrap_or(50);
    let aggregator = Arc::new(IntelligenceAggregator::new(pool.get_ref().clone(), Arc::new(config.get_ref().clone())));
    let detector = EventDetector::new(pool.get_ref().clone(), aggregator);
    match detector.get_recent_events(limit).await {
        Ok(events) => HttpResponse::Ok().json(events),
        Err(e) => {
            error!("Failed to get intelligence events: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Whales ──────────────────────────────────────────────────────────────────

/// GET /api/intelligence/whales — recent large on-chain transactions
pub async fn get_whales(pool: web::Data<PgPool>, query: web::Query<HistoryQuery>, config: web::Data<Config>) -> impl Responder {
    let limit = query.limit.unwrap_or(50);
    let tracker = WhaleTracker::new(pool.get_ref().clone(), config.whale_alert_api_key.clone());
    match tracker.get_recent_alerts(limit).await {
        Ok(alerts) => HttpResponse::Ok().json(alerts),
        Err(e) => {
            error!("Failed to get whale alerts: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Arb (live) ──────────────────────────────────────────────────────────────

/// GET /api/intelligence/arb — live open arb opportunities
pub async fn get_arb_live(pool: web::Data<PgPool>) -> impl Responder {
    let scanner = ArbScanner::new(pool.get_ref().clone());
    match scanner.get_live_opportunities().await {
        Ok(opps) => HttpResponse::Ok().json(opps),
        Err(e) => {
            error!("Failed to get arb opportunities: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

/// GET /api/intelligence/arb/history — all arb opportunities (closed + open)
pub async fn get_arb_history(pool: web::Data<PgPool>, query: web::Query<HistoryQuery>) -> impl Responder {
    let limit = query.limit.unwrap_or(100);
    let scanner = ArbScanner::new(pool.get_ref().clone());
    match scanner.get_opportunity_history(limit).await {
        Ok(opps) => HttpResponse::Ok().json(opps),
        Err(e) => {
            error!("Failed to get arb history: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Score History ────────────────────────────────────────────────────────────

/// GET /api/intelligence/history/{instrument} — score history for instrument
pub async fn get_score_history(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
    query: web::Query<HistoryQuery>,
) -> impl Responder {
    let instrument = path.into_inner();
    let hours = query.hours.unwrap_or(24);
    let scorer = IntelligenceScorer::new(pool.get_ref().clone());
    match scorer.get_score_history(&instrument, hours).await {
        Ok(history) => HttpResponse::Ok().json(history),
        Err(e) => {
            error!("Failed to get score history for {}: {}", instrument, e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/intelligence")
            .route("/scores", web::get().to(get_scores))
            .route("/feeds", web::get().to(get_feeds))
            .route("/events", web::get().to(get_events))
            .route("/whales", web::get().to(get_whales))
            .route("/arb", web::get().to(get_arb_live))
            .route("/arb/history", web::get().to(get_arb_history))
            .route("/history/{instrument}", web::get().to(get_score_history))
            .route("/gate-thresholds", web::get().to(get_gate_thresholds_handler))
            .route("/gate-thresholds", web::post().to(set_gate_threshold_handler))
            .route("/polymarket-settings", web::get().to(get_polymarket_settings_handler))
            .route("/polymarket-settings", web::post().to(set_polymarket_setting_handler)),
    );
}

// ─── Gate Thresholds ─────────────────────────────────────────────────────────

/// GET /api/intelligence/gate-thresholds
pub async fn get_gate_thresholds_handler(pool: web::Data<PgPool>) -> impl Responder {
    let thresholds = super::settings::get_gate_thresholds(pool.get_ref()).await;
    HttpResponse::Ok().json(thresholds)
}

#[derive(Deserialize)]
pub struct SetThresholdRequest {
    pub key: String,
    pub value: f64,
}

/// POST /api/intelligence/gate-thresholds
pub async fn set_gate_threshold_handler(
    pool: web::Data<PgPool>,
    body: web::Json<SetThresholdRequest>,
) -> impl Responder {
    match super::settings::set_gate_threshold(pool.get_ref(), &body.key, body.value).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => {
            error!("Failed to set gate threshold: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Polymarket Settings ─────────────────────────────────────────────────────

/// GET /api/intelligence/polymarket-settings
pub async fn get_polymarket_settings_handler(pool: web::Data<PgPool>) -> impl Responder {
    let settings = super::settings::get_polymarket_settings(pool.get_ref()).await;
    HttpResponse::Ok().json(settings)
}

#[derive(Deserialize)]
pub struct SetPolymarketSettingRequest {
    pub key: String,
    pub value: String,
}

/// POST /api/intelligence/polymarket-settings
pub async fn set_polymarket_setting_handler(
    pool: web::Data<PgPool>,
    body: web::Json<SetPolymarketSettingRequest>,
) -> impl Responder {
    match super::settings::set_polymarket_setting(pool.get_ref(), &body.key, &body.value).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => {
            error!("Failed to set polymarket setting: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}
