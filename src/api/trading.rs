use actix_web::{web, HttpResponse, Result};
use uuid::Uuid;
use std::sync::Mutex;
use std::collections::HashMap;

use crate::broker::OandaClient;
use crate::config::Config;
use crate::error::AppError;
use crate::models::{BotState, BotStatus, Position, TradeDirection};

// In-memory bot state (in production, this would be more sophisticated)
lazy_static::lazy_static! {
    static ref BOT_STATE: Mutex<BotState> = Mutex::new(BotState::default());
}

#[derive(serde::Deserialize)]
pub struct StartBotRequest {
    #[serde(default)]
    strategy_id: Option<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/trading")
            .route("/start", web::post().to(start_bot))
            .route("/stop", web::post().to(stop_bot))
            .route("/pause", web::post().to(pause_bot))
            .route("/status", web::get().to(get_status))
            .route("/account", web::get().to(get_account))
            .route("/positions", web::get().to(get_positions))
            .route("/positions/{id}/close", web::post().to(close_position))
            .route("/trades", web::get().to(get_trades))
            .route("/trades/open", web::get().to(get_open_trades))
            .route("/trades/closed", web::get().to(get_closed_trades))
    );
}

async fn start_bot(
    body: web::Json<StartBotRequest>,
) -> Result<HttpResponse, AppError> {
    let mut state = BOT_STATE.lock().unwrap();
    
    if state.status == BotStatus::Running {
        return Err(AppError::BadRequest("Bot is already running".to_string()));
    }
    
    state.status = BotStatus::Running;
    state.active_strategy_id = body.strategy_id.as_ref().and_then(|s| Uuid::parse_str(s).ok());
    state.started_at = Some(chrono::Utc::now());
    
    // TODO: Actually start the trading engine in a background task
    
    Ok(HttpResponse::Ok().json(state.clone()))
}

async fn stop_bot() -> Result<HttpResponse, AppError> {
    let mut state = BOT_STATE.lock().unwrap();
    
    state.status = BotStatus::Stopped;
    state.active_strategy_id = None;
    state.started_at = None;
    
    // TODO: Actually stop the trading engine
    
    Ok(HttpResponse::Ok().json(state.clone()))
}

async fn pause_bot() -> Result<HttpResponse, AppError> {
    let mut state = BOT_STATE.lock().unwrap();
    
    if state.status != BotStatus::Running {
        return Err(AppError::BadRequest("Bot is not running".to_string()));
    }
    
    state.status = BotStatus::Paused;
    
    Ok(HttpResponse::Ok().json(state.clone()))
}

async fn get_status() -> Result<HttpResponse, AppError> {
    let state = BOT_STATE.lock().unwrap();
    Ok(HttpResponse::Ok().json(state.clone()))
}

async fn get_account(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let summary = client.get_account_summary().await?;
        
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "balance": summary.balance.parse::<f64>().unwrap_or(0.0),
            "currency": summary.currency,
            "profit_loss": summary.profit_loss.parse::<f64>().unwrap_or(0.0),
            "unrealized_pnl": summary.unrealized_pl.parse::<f64>().unwrap_or(0.0),
            "nav": summary.nav.parse::<f64>().unwrap_or(0.0),
            "margin_used": summary.margin_used.parse::<f64>().unwrap_or(0.0),
            "margin_available": summary.margin_available.parse::<f64>().unwrap_or(0.0),
            "open_trade_count": summary.open_trade_count,
            "open_position_count": summary.open_position_count
        })));
    }
    
    // Mock account if OANDA not configured
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "balance": 50000.0,
        "currency": "USD",
        "profit_loss": 1234.56,
        "unrealized_pnl": 0.0,
        "nav": 50000.0,
        "margin_used": 0.0,
        "margin_available": 50000.0,
        "open_trade_count": 0,
        "open_position_count": 0
    })))
}

async fn get_positions(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let oanda_positions = client.get_positions().await?;
        
        // Convert OANDA positions to our Position format
        let positions: Vec<Position> = oanda_positions
            .iter()
            .filter_map(|p| {
                let (direction, units, avg_price, unrealized_pnl) = if p.long.units.parse::<f64>().unwrap_or(0.0) > 0.0 {
                    (
                        TradeDirection::Long,
                        p.long.units.parse::<f64>().unwrap_or(0.0),
                        p.long.average_price.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                        p.long.unrealized_pl.parse::<f64>().unwrap_or(0.0),
                    )
                } else if p.short.units.parse::<f64>().unwrap_or(0.0).abs() > 0.0 {
                    (
                        TradeDirection::Short,
                        p.short.units.parse::<f64>().unwrap_or(0.0).abs(),
                        p.short.average_price.as_ref().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                        p.short.unrealized_pl.parse::<f64>().unwrap_or(0.0),
                    )
                } else {
                    return None;
                };
                
                // Convert OANDA symbol format (XAU_USD -> XAUUSD)
                let symbol = p.instrument.replace("_", "");
                
                Some(Position {
                    id: Uuid::new_v4(),
                    symbol,
                    direction,
                    entry_price: avg_price,
                    current_price: avg_price, // We'll update this with real price
                    quantity: units,
                    unrealized_pnl,
                    opened_at: chrono::Utc::now(),
                })
            })
            .collect();
        
        return Ok(HttpResponse::Ok().json(positions));
    }
    
    // Return empty positions if OANDA not configured
    let positions: Vec<Position> = vec![];
    Ok(HttpResponse::Ok().json(positions))
}

async fn close_position(
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    // TODO: Actually close the position with the broker
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Position closed",
        "position_id": id
    })))
}

#[derive(serde::Deserialize)]
pub struct TradeQuery {
    count: Option<u32>,
}

async fn get_trades(
    config: web::Data<Config>,
    query: web::Query<TradeQuery>,
) -> Result<HttpResponse, AppError> {
    let count = query.count.unwrap_or(20);
    
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let trades = client.get_trades(count, None).await?;
        
        // Format trades for display
        let formatted: Vec<serde_json::Value> = trades.iter().map(|t| {
            let units: f64 = t.initial_units.parse().unwrap_or(0.0);
            let direction = if units > 0.0 { "LONG" } else { "SHORT" };
            let realized_pl: f64 = t.realized_pl.as_ref()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0.0);
            let unrealized_pl: f64 = t.unrealized_pl.as_ref()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0.0);
            
            serde_json::json!({
                "id": t.id,
                "instrument": t.instrument,
                "direction": direction,
                "units": units.abs(),
                "entry_price": t.price,
                "exit_price": t.average_close_price,
                "state": t.state,
                "open_time": t.open_time,
                "close_time": t.close_time,
                "realized_pl": realized_pl,
                "unrealized_pl": unrealized_pl,
                "stop_loss": t.stop_loss_order.as_ref().map(|sl| &sl.price),
                "take_profit": t.take_profit_order.as_ref().map(|tp| &tp.price),
            })
        }).collect();
        
        return Ok(HttpResponse::Ok().json(formatted));
    }
    
    Err(AppError::InternalError("OANDA not configured".to_string()))
}

async fn get_open_trades(
    config: web::Data<Config>,
    pool: web::Data<sqlx::PgPool>,
) -> Result<HttpResponse, AppError> {
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let trades = client.get_open_trades().await?;
        
        // Fetch bot names for each trade from trade_context
        let bot_map: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT tc.external_trade_id, b.name
            FROM trade_context tc
            JOIN bots b ON tc.bot_id = b.id
            WHERE tc.closed_at IS NULL AND tc.external_trade_id IS NOT NULL
            "#
        )
        .fetch_all(pool.get_ref())
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();
        
        // Enrich trades with bot names
        let enriched: Vec<serde_json::Value> = trades.iter().map(|t| {
            let bot_name = bot_map.get(&t.id).cloned();
            serde_json::json!({
                "id": t.id,
                "instrument": t.instrument,
                "price": t.price,
                "openTime": t.open_time,
                "currentUnits": t.current_units,
                "unrealizedPL": t.unrealized_pl,
                "stopLossOrder": t.stop_loss_order,
                "takeProfitOrder": t.take_profit_order,
                "botName": bot_name,
            })
        }).collect();
        
        return Ok(HttpResponse::Ok().json(enriched));
    }
    
    Err(AppError::InternalError("OANDA not configured".to_string()))
}

async fn get_closed_trades(
    config: web::Data<Config>,
    query: web::Query<TradeQuery>,
) -> Result<HttpResponse, AppError> {
    let count = query.count.unwrap_or(20);
    
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let trades = client.get_closed_trades(count).await?;
        
        // Format with P&L summary
        let formatted: Vec<serde_json::Value> = trades.iter().map(|t| {
            let units: f64 = t.initial_units.parse().unwrap_or(0.0);
            let direction = if units > 0.0 { "LONG" } else { "SHORT" };
            let realized_pl: f64 = t.realized_pl.as_ref()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0.0);
            
            serde_json::json!({
                "id": t.id,
                "instrument": t.instrument,
                "direction": direction,
                "units": units.abs(),
                "entry_price": t.price,
                "exit_price": t.average_close_price,
                "open_time": t.open_time,
                "close_time": t.close_time,
                "realized_pl": realized_pl,
                "won": realized_pl > 0.0,
            })
        }).collect();
        
        // Summary stats
        let total_pl: f64 = formatted.iter()
            .filter_map(|t| t["realized_pl"].as_f64())
            .sum();
        let wins = formatted.iter().filter(|t| t["won"].as_bool().unwrap_or(false)).count();
        let losses = formatted.len() - wins;
        
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "trades": formatted,
            "summary": {
                "total_trades": formatted.len(),
                "wins": wins,
                "losses": losses,
                "win_rate": if formatted.is_empty() { 0.0 } else { wins as f64 / formatted.len() as f64 * 100.0 },
                "total_pl": total_pl,
            }
        })));
    }
    
    Err(AppError::InternalError("OANDA not configured".to_string()))
}
