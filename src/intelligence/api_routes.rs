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
            .route("/polymarket-settings", web::post().to(set_polymarket_setting_handler))
            .route("/global-tension", web::get().to(get_global_tension))
            .route("/feed/posts", web::get().to(get_feed_posts))
            .route("/markets/movers", web::get().to(get_market_movers))
            .route("/markets", web::get().to(get_markets))
            .route("/feed/sources", web::get().to(get_feed_sources))
            .route("/feed/sources", web::post().to(add_feed_source))
            .route("/feed/sources/{id}", web::delete().to(delete_feed_source))
            .route("/feed/sources/{id}", web::patch().to(toggle_feed_source))
            .route("/flights", web::get().to(get_flights))
            .route("/signals", web::get().to(get_signals))
            .route("/signals/history", web::get().to(get_signal_history_handler))
            .route("/signals/{id}/close", web::patch().to(close_signal_handler))
            .route("/whales/wallets", web::get().to(get_whale_wallets))
            .route("/whales/wallets", web::post().to(add_whale_wallet))
            .route("/whales/wallets/{address}", web::delete().to(remove_whale_wallet))
            .route("/whales/trades", web::get().to(get_whale_trades))
            .route("/whales/corroborations", web::get().to(get_whale_corroborations)),
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

// ─── Feed Sources & Posts ─────────────────────────────────────────────────────

use super::feed;
use uuid::Uuid;

#[derive(Deserialize)]
struct FeedPostsQuery {
    limit: Option<i64>,
    source: Option<String>,
    severity: Option<String>,
    category: Option<String>,
}

/// GET /api/intelligence/feed/posts?limit=50&source=@handle&severity=HIGH
pub async fn get_feed_posts(
    pool: web::Data<PgPool>,
    query: web::Query<FeedPostsQuery>,
) -> impl Responder {
    let limit = query.limit.unwrap_or(50).min(200);
    match feed::get_feed_posts(
        pool.get_ref(),
        limit,
        query.source.as_deref(),
        query.severity.as_deref(),
        query.category.as_deref(),
    )
    .await
    {
        Ok(posts) => HttpResponse::Ok().json(posts),
        Err(e) => {
            error!("Failed to get feed posts: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

/// GET /api/intelligence/feed/sources
pub async fn get_feed_sources(pool: web::Data<PgPool>) -> impl Responder {
    match feed::get_feed_sources(pool.get_ref()).await {
        Ok(sources) => HttpResponse::Ok().json(sources),
        Err(e) => {
            error!("Failed to get feed sources: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

#[derive(Deserialize)]
struct AddSourceRequest {
    source_type: String,
    name: String,
    handle: String,
    #[serde(default = "default_severity")]
    default_severity: String,
}
fn default_severity() -> String { "MEDIUM".to_string() }

/// POST /api/intelligence/feed/sources
pub async fn add_feed_source(
    pool: web::Data<PgPool>,
    body: web::Json<AddSourceRequest>,
) -> impl Responder {
    match feed::add_feed_source(
        pool.get_ref(),
        &body.source_type,
        &body.name,
        &body.handle,
        &body.default_severity,
    )
    .await
    {
        Ok(src) => HttpResponse::Created().json(src),
        Err(e) => {
            error!("Failed to add feed source: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

/// DELETE /api/intelligence/feed/sources/{id}
pub async fn delete_feed_source(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> impl Responder {
    match feed::delete_feed_source(pool.get_ref(), path.into_inner()).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            error!("Failed to delete feed source: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

#[derive(Deserialize)]
struct ToggleSourceRequest {
    enabled: bool,
}

/// PATCH /api/intelligence/feed/sources/{id}
pub async fn toggle_feed_source(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<ToggleSourceRequest>,
) -> impl Responder {
    match feed::toggle_feed_source(pool.get_ref(), path.into_inner(), body.enabled).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => {
            error!("Failed to toggle feed source: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Global Tension ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GlobalTensionResponse {
    score: f64,
    label: &'static str,
    instruments: Vec<InstrumentTension>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
struct InstrumentTension {
    instrument: String,
    tension_score: f64,
    direction_bias: String,
}

/// GET /api/intelligence/global-tension
/// Returns the aggregate tension index (mean of all instrument tension scores).
pub async fn get_global_tension(pool: web::Data<PgPool>) -> impl Responder {
    let scorer = IntelligenceScorer::new(pool.get_ref().clone());
    match scorer.get_latest_scores().await {
        Ok(scores) if !scores.is_empty() => {
            let avg = scores.iter().map(|s| s.tension_score).sum::<f64>() / scores.len() as f64;
            let label = if avg >= 75.0 { "SEVERE" } else if avg >= 50.0 { "HIGH" } else if avg >= 25.0 { "GUARDED" } else { "CALM" };
            let instruments = scores.into_iter().map(|s| InstrumentTension {
                instrument: s.instrument,
                tension_score: s.tension_score,
                direction_bias: s.direction_bias.unwrap_or_default(),
            }).collect();
            HttpResponse::Ok().json(GlobalTensionResponse {
                score: avg,
                label,
                instruments,
                updated_at: chrono::Utc::now(),
            })
        }
        Ok(_) => {
            // No scores yet — return default CALM
            HttpResponse::Ok().json(GlobalTensionResponse {
                score: 30.0,
                label: "CALM",
                instruments: vec![],
                updated_at: chrono::Utc::now(),
            })
        }
        Err(e) => {
            error!("Failed to get global tension: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Flights ──────────────────────────────────────────────────────────────────

use super::flights;

#[derive(Deserialize)]
struct FlightsQuery {
    #[serde(rename = "type")]
    aircraft_type: Option<String>,
    limit: Option<i64>,
}

/// GET /api/intelligence/flights?type=ISR&limit=100
pub async fn get_flights(
    pool: web::Data<PgPool>,
    query: web::Query<FlightsQuery>,
) -> impl Responder {
    let limit = query.limit.unwrap_or(200).min(500);
    let type_filter = query.aircraft_type.as_deref()
        .filter(|t| *t != "ALL" && !t.is_empty());

    match flights::get_tracked_flights(pool.get_ref(), type_filter, limit).await {
        Ok(f) => HttpResponse::Ok().json(f),
        Err(e) => {
            error!("Failed to get flights: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Markets ─────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct MarketsQuery {
    category: Option<String>,
    sort:     Option<String>,
    limit:    Option<usize>,
}

/// GET /api/intelligence/markets?category=politics&sort=volume&limit=50
pub async fn get_markets(
    query: web::Query<MarketsQuery>,
) -> impl Responder {
    use super::markets::MarketsFetcher;
    let fetcher  = MarketsFetcher::new();
    let category = query.category.as_deref();
    let sort     = query.sort.as_deref().unwrap_or("volume");
    let limit    = query.limit.unwrap_or(50).min(200);

    match fetcher.fetch(category, sort, limit).await {
        Ok(cards) => HttpResponse::Ok().json(cards),
        Err(e) => {
            error!("Markets fetch failed: {}", e);
            HttpResponse::ServiceUnavailable().json(ErrorResponse { error: e })
        }
    }
}

// ─── Market Movers ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct MoversQuery {
    hours: Option<i32>,
    limit: Option<i64>,
}

/// GET /api/intelligence/markets/movers?hours=24&limit=20
pub async fn get_market_movers(
    query: web::Query<MoversQuery>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    use super::market_movers::MarketSnapshotTracker;
    let tracker = MarketSnapshotTracker::new(pool.get_ref().clone());
    let hours = query.hours.unwrap_or(24).max(1).min(72);
    let limit = query.limit.unwrap_or(20).max(1).min(50);
    match tracker.get_movers(hours, limit).await {
        Ok(movers) => HttpResponse::Ok().json(movers),
        Err(e) => {
            error!("Market movers query failed: {}", e);
            HttpResponse::ServiceUnavailable().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Signal Intelligence ─────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SignalsQuery {
    hours: Option<i32>,
    limit: Option<i64>,
}

/// GET /api/intelligence/signals?hours=6&limit=20
pub async fn get_signals(
    query: web::Query<SignalsQuery>,
    pool: web::Data<PgPool>,
) -> impl Responder {
    use super::signal_intelligence::SignalEngine;
    let engine = SignalEngine::new(pool.get_ref().clone());
    let hours  = query.hours.unwrap_or(6).max(1).min(48);
    let limit  = query.limit.unwrap_or(20).max(1).min(50);
    match engine.get_signals(hours, limit).await {
        Ok(signals) => HttpResponse::Ok().json(signals),
        Err(e) => {
            error!("Signal intelligence query failed: {}", e);
            HttpResponse::ServiceUnavailable().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Signal History ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct SignalHistoryQuery {
    limit: Option<i64>,
}

/// GET /api/intelligence/signals/history?limit=50
pub async fn get_signal_history_handler(
    query: web::Query<SignalHistoryQuery>,
    pool:  web::Data<PgPool>,
) -> impl Responder {
    use super::signal_tracker::SignalTracker;
    let tracker = SignalTracker::new(pool.get_ref().clone());
    let limit   = query.limit.unwrap_or(50).max(1).min(200);
    match tracker.get_signal_history(limit).await {
        Ok(signals) => HttpResponse::Ok().json(signals),
        Err(e) => {
            error!("Signal history query failed: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

/// PATCH /api/intelligence/signals/{id}/close?exit_price=0.42
#[derive(serde::Deserialize)]
struct CloseSignalQuery {
    exit_price: f64,
}

pub async fn close_signal_handler(
    path:  web::Path<uuid::Uuid>,
    query: web::Query<CloseSignalQuery>,
    pool:  web::Data<PgPool>,
) -> impl Responder {
    use super::signal_tracker::SignalTracker;
    let tracker = SignalTracker::new(pool.get_ref().clone());
    match tracker.close_signal(path.into_inner(), query.exit_price).await {
        Ok(_)  => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => {
            error!("Close signal failed: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() })
        }
    }
}

// ─── Whale Tracker Routes ─────────────────────────────────────────────────────

use super::polymarket_whales::WhaleFetcher;

/// GET /api/intelligence/whales/wallets
pub async fn get_whale_wallets(pool: web::Data<PgPool>) -> impl Responder {
    let fetcher = WhaleFetcher::new(pool.get_ref().clone());
    match fetcher.get_wallets().await {
        Ok(wallets) => HttpResponse::Ok().json(wallets),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
    }
}

#[derive(serde::Deserialize)]
struct AddWalletRequest {
    address:          String,
    alias:            Option<String>,
    win_rate_pct:     Option<f64>,
    total_pnl_usd:    Option<f64>,
    account_age_days: Option<i32>,
}

/// POST /api/intelligence/whales/wallets
pub async fn add_whale_wallet(
    pool: web::Data<PgPool>,
    body: web::Json<AddWalletRequest>,
) -> impl Responder {
    let fetcher = WhaleFetcher::new(pool.get_ref().clone());
    let alias   = body.alias.as_deref().unwrap_or(&body.address[..body.address.len().min(14)]);
    match fetcher.add_wallet(
        &body.address,
        alias,
        body.win_rate_pct.unwrap_or(0.0),
        body.total_pnl_usd.unwrap_or(0.0),
        body.account_age_days.unwrap_or(0),
    ).await {
        Ok(_)  => HttpResponse::Created().json(serde_json::json!({ "success": true })),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
    }
}

/// DELETE /api/intelligence/whales/wallets/{address}
pub async fn remove_whale_wallet(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> impl Responder {
    let fetcher = WhaleFetcher::new(pool.get_ref().clone());
    match fetcher.remove_wallet(&path.into_inner()).await {
        Ok(_)  => HttpResponse::Ok().json(serde_json::json!({ "success": true })),
        Err(e) => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
    }
}

#[derive(serde::Deserialize)]
struct WhaleTradesQuery {
    limit: Option<i64>,
}

/// GET /api/intelligence/whales/trades?limit=50
pub async fn get_whale_trades(
    pool:  web::Data<PgPool>,
    query: web::Query<WhaleTradesQuery>,
) -> impl Responder {
    let fetcher = WhaleFetcher::new(pool.get_ref().clone());
    let limit   = query.limit.unwrap_or(50).min(200);
    match fetcher.get_recent_trades(limit, false).await {
        Ok(trades) => HttpResponse::Ok().json(trades),
        Err(e)     => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
    }
}

/// GET /api/intelligence/whales/corroborations
pub async fn get_whale_corroborations(
    pool:  web::Data<PgPool>,
    query: web::Query<WhaleTradesQuery>,
) -> impl Responder {
    let fetcher = WhaleFetcher::new(pool.get_ref().clone());
    let limit   = query.limit.unwrap_or(50).min(200);
    match fetcher.get_recent_trades(limit, true).await {
        Ok(trades) => HttpResponse::Ok().json(trades),
        Err(e)     => HttpResponse::InternalServerError().json(ErrorResponse { error: e.to_string() }),
    }
}
