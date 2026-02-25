use actix_web::{web, HttpResponse, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::bot::activity::{get_recent_activities, get_all_active_activities, BotActivityResponse};
use crate::bot::runner::BotManager;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{Bot, CreateBotRequest, UpdateBotRequest, BotResponse, Strategy};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/bots")
            .route("", web::get().to(list_bots))
            .route("", web::post().to(create_bot))
            .route("/{id}", web::get().to(get_bot))
            .route("/{id}", web::put().to(update_bot))
            .route("/{id}", web::delete().to(delete_bot))
            .route("/{id}/start", web::post().to(start_bot))
            .route("/{id}/stop", web::post().to(stop_bot))
            .route("/{id}/restart", web::post().to(restart_bot))
            .route("/{id}/activity", web::get().to(get_bot_activity))
            .route("/{id}/cooldown", web::get().to(get_cooldown_status))
            .route("/activity/all", web::get().to(get_all_activity))
            .route("/{id}/intelligence-gate", web::get().to(get_bot_gate))
            .route("/{id}/intelligence-gate", web::post().to(set_bot_gate))
    );
}

async fn list_bots(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    // For now, list all bots
    let bots = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots ORDER BY created_at DESC"
    )
    .fetch_all(pool.get_ref())
    .await?;

    // Get strategy names
    let mut responses: Vec<BotResponse> = Vec::new();
    for bot in bots {
        let mut response: BotResponse = bot.clone().into();
        
        if let Some(strategy_id) = bot.strategy_id {
            if let Ok(Some(strategy)) = sqlx::query_as::<_, Strategy>(
                "SELECT * FROM strategies WHERE id = $1"
            )
            .bind(strategy_id)
            .fetch_optional(pool.get_ref())
            .await {
                response.strategy_name = Some(strategy.name);
                // Get timeframe from strategy params
                response.timeframe = strategy.params.get("timeframe")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
        responses.push(response);
    }

    Ok(HttpResponse::Ok().json(responses))
}

async fn create_bot(
    pool: web::Data<DbPool>,
    body: web::Json<CreateBotRequest>,
) -> Result<HttpResponse, AppError> {
    let user_id = Uuid::parse_str("a0000000-0000-0000-0000-000000000001").unwrap(); // TODO: JWT
    let lot_size = body.lot_size.unwrap_or(0.01);
    
    let bot = sqlx::query_as::<_, Bot>(
        r#"
        INSERT INTO bots (user_id, name, strategy_id, broker, account_type, api_token, account_id, 
                          lot_size, max_positions, max_daily_trades, max_daily_loss_pct)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING *
        "#
    )
    .bind(user_id)
    .bind(&body.name)
    .bind(body.strategy_id)
    .bind(body.broker.as_deref().unwrap_or("oanda"))
    .bind(&body.account_type)
    .bind(&body.api_token)
    .bind(&body.account_id)
    .bind(lot_size)
    .bind(body.max_positions.unwrap_or(1))
    .bind(body.max_daily_trades)
    .bind(body.max_daily_loss_pct)
    .fetch_one(pool.get_ref())
    .await?;

    let response: BotResponse = bot.into();
    Ok(HttpResponse::Created().json(response))
}

async fn get_bot(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    let bot = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    let mut response: BotResponse = bot.clone().into();
    
    if let Some(strategy_id) = bot.strategy_id {
        if let Ok(Some(strategy)) = sqlx::query_as::<_, Strategy>(
            "SELECT * FROM strategies WHERE id = $1"
        )
        .bind(strategy_id)
        .fetch_optional(pool.get_ref())
        .await {
            response.strategy_name = Some(strategy.name);
            response.timeframe = strategy.params.get("timeframe")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }

    Ok(HttpResponse::Ok().json(response))
}

async fn update_bot(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateBotRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    let existing = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    let name = body.name.clone().unwrap_or(existing.name);
    let strategy_id = body.strategy_id.or(existing.strategy_id);
    let account_type = body.account_type.clone().unwrap_or(existing.account_type);
    let api_token = body.api_token.clone().or(existing.api_token);
    let account_id = body.account_id.clone().or(existing.account_id);
    let lot_size = body.lot_size.unwrap_or(existing.lot_size);
    let max_positions = body.max_positions.unwrap_or(existing.max_positions);
    let cooldown_seconds = body.cooldown_seconds.or(existing.cooldown_seconds);
    let is_active = body.is_active.unwrap_or(existing.is_active);

    let bot = sqlx::query_as::<_, Bot>(
        r#"
        UPDATE bots SET 
            name = $1, strategy_id = $2, account_type = $3, api_token = $4,
            account_id = $5, lot_size = $6, max_positions = $7, cooldown_seconds = $8,
            is_active = $9, updated_at = $10
        WHERE id = $11
        RETURNING *
        "#
    )
    .bind(&name)
    .bind(strategy_id)
    .bind(&account_type)
    .bind(&api_token)
    .bind(&account_id)
    .bind(lot_size)
    .bind(max_positions)
    .bind(cooldown_seconds)
    .bind(is_active)
    .bind(Utc::now())
    .bind(id)
    .fetch_one(pool.get_ref())
    .await?;

    let response: BotResponse = bot.into();
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_bot(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    let result = sqlx::query("DELETE FROM bots WHERE id = $1")
        .bind(id)
        .execute(pool.get_ref())
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Bot not found".to_string()));
    }
    
    Ok(HttpResponse::NoContent().finish())
}

async fn start_bot(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    bot_manager: web::Data<BotManager>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    // Check bot exists
    let bot = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    // Check it has required config
    if bot.strategy_id.is_none() {
        return Err(AppError::BadRequest("Bot must have a strategy assigned".to_string()));
    }
    
    // Use bot credentials or fall back to global config
    let has_bot_creds = bot.account_id.is_some() && bot.api_token.is_some();
    let has_global_creds = config.oanda_account_id.is_some() && config.oanda_api_token.is_some();
    
    if !has_bot_creds && !has_global_creds {
        return Err(AppError::BadRequest(
            "No broker credentials configured. Set them on the bot or in OANDA_API_TOKEN/OANDA_ACCOUNT_ID env vars.".to_string()
        ));
    }

    // Update status in DB
    sqlx::query(
        "UPDATE bots SET is_active = true, last_started_at = $1, updated_at = $1 WHERE id = $2"
    )
    .bind(Utc::now())
    .bind(id)
    .execute(pool.get_ref())
    .await?;

    // Start the actual bot trading loop
    if let Err(e) = bot_manager.start_bot(id).await {
        // Revert DB status if bot failed to start
        let _ = sqlx::query("UPDATE bots SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(pool.get_ref())
            .await;
        return Err(AppError::InternalError(format!("Failed to start bot: {}", e)));
    }

    let creds_source = if has_bot_creds { "bot config" } else { "global OANDA config" };
    tracing::info!("🤖 Bot {} started using credentials from {}", id, creds_source);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": format!("Bot started (using {})", creds_source),
        "bot_id": id
    })))
}

async fn stop_bot(
    pool: web::Data<DbPool>,
    bot_manager: web::Data<BotManager>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    // Update DB status
    sqlx::query(
        "UPDATE bots SET is_active = false, last_stopped_at = $1, updated_at = $1 WHERE id = $2"
    )
    .bind(Utc::now())
    .bind(id)
    .execute(pool.get_ref())
    .await?;

    // Stop the bot trading loop (the loop will exit on next tick when it sees is_active = false)
    let _ = bot_manager.stop_bot(id).await;
    
    tracing::info!("🛑 Bot {} stopped", id);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Bot stopped",
        "bot_id": id
    })))
}

async fn restart_bot(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    bot_manager: web::Data<BotManager>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    // Check bot exists and has required config
    let bot = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    if bot.strategy_id.is_none() {
        return Err(AppError::BadRequest("Bot must have a strategy assigned".to_string()));
    }

    // Stop the bot first
    let _ = bot_manager.stop_bot(id).await;
    
    // Small delay to ensure clean shutdown
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Update DB status and restart
    sqlx::query(
        "UPDATE bots SET is_active = true, last_started_at = $1, updated_at = $1 WHERE id = $2"
    )
    .bind(Utc::now())
    .bind(id)
    .execute(pool.get_ref())
    .await?;

    // Start the bot again
    if let Err(e) = bot_manager.start_bot(id).await {
        // Revert DB status if bot failed to start
        let _ = sqlx::query("UPDATE bots SET is_active = false WHERE id = $1")
            .bind(id)
            .execute(pool.get_ref())
            .await;
        return Err(AppError::InternalError(format!("Failed to restart bot: {}", e)));
    }

    tracing::info!("🔄 Bot {} restarted", id);

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Bot restarted",
        "bot_id": id
    })))
}

/// Get recent activity for a specific bot
async fn get_bot_activity(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    query: web::Query<ActivityQuery>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    let limit = query.limit.unwrap_or(50).min(100);
    
    let activities = get_recent_activities(pool.get_ref(), bot_id, limit as i64)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    
    let responses: Vec<BotActivityResponse> = activities.into_iter().map(|a| a.into()).collect();
    
    Ok(HttpResponse::Ok().json(responses))
}

/// Get recent activity for all active bots (for the status widget)
async fn get_all_activity(
    pool: web::Data<DbPool>,
    query: web::Query<ActivityQuery>,
) -> Result<HttpResponse, AppError> {
    let limit = query.limit.unwrap_or(20).min(100);
    
    let activities = get_all_active_activities(pool.get_ref(), limit as i64)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    
    let responses: Vec<BotActivityResponse> = activities.into_iter().map(|a| a.into()).collect();
    
    Ok(HttpResponse::Ok().json(responses))
}

#[derive(Debug, serde::Deserialize)]
struct ActivityQuery {
    limit: Option<u32>,
}

/// Get cooldown status for a bot
async fn get_cooldown_status(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    
    // Get bot's cooldown setting (or fall back to strategy, then default)
    let bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = $1")
        .bind(bot_id)
        .fetch_optional(pool.get_ref())
        .await?
        .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;
    
    // Priority: bot setting > strategy param > default (300s)
    let mut cooldown_seconds: i64 = 300;
    
    if let Some(bot_cooldown) = bot.cooldown_seconds {
        cooldown_seconds = bot_cooldown as i64;
    } else if let Some(strategy_id) = bot.strategy_id {
        // Try to get from strategy params
        if let Ok(Some(strategy)) = sqlx::query_as::<_, Strategy>(
            "SELECT * FROM strategies WHERE id = $1"
        )
        .bind(strategy_id)
        .fetch_optional(pool.get_ref())
        .await {
            if let Some(cd) = strategy.params.get("cooldown_seconds").and_then(|v| v.as_i64()) {
                cooldown_seconds = cd;
            }
        }
    }
    
    // Find the last order_placed activity
    let last_trade = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        r#"
        SELECT created_at FROM bot_activities 
        WHERE bot_id = $1 AND activity_type = 'order_placed'
        ORDER BY created_at DESC 
        LIMIT 1
        "#
    )
    .bind(bot_id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;
    
    let (in_cooldown, remaining_seconds, last_trade_at) = if let Some(last_trade_time) = last_trade {
        let elapsed = (Utc::now() - last_trade_time).num_seconds();
        let remaining = (cooldown_seconds - elapsed).max(0);
        (remaining > 0, remaining, Some(last_trade_time.to_rfc3339()))
    } else {
        (false, 0, None)
    };
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "in_cooldown": in_cooldown,
        "remaining_seconds": remaining_seconds,
        "cooldown_seconds": cooldown_seconds,
        "last_trade_at": last_trade_at,
    })))
}

// ─── Intelligence Gate Toggle ─────────────────────────────────────────────────

async fn get_bot_gate(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    let enabled = crate::intelligence::settings::get_bot_intelligence_gate(pool.get_ref(), bot_id).await;
    let thresholds = crate::intelligence::settings::get_gate_thresholds(pool.get_ref()).await;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "bot_id": bot_id,
        "intelligence_gate_enabled": enabled,
        "thresholds": thresholds,
    })))
}

#[derive(serde::Deserialize)]
struct SetGateRequest {
    enabled: bool,
}

async fn set_bot_gate(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<SetGateRequest>,
) -> Result<HttpResponse, AppError> {
    let bot_id = path.into_inner();
    crate::intelligence::settings::set_bot_intelligence_gate(pool.get_ref(), bot_id, body.enabled)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "bot_id": bot_id,
        "intelligence_gate_enabled": body.enabled,
        "message": if body.enabled { "Intelligence gate ACTIVE — will block/reduce/boost trades" } else { "Intelligence gate OBSERVE — logging impact only" },
    })))
}
