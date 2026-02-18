use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use lazy_static::lazy_static;
use grammers_client::Client;

use crate::error::AppError;
use crate::telegram::{
    TradeSignal, SignalParser, ChannelMonitor, SignalAnalysis, 
    MtprotoClient, AuthState, HistoricalSignal, StrategyDiscovery, DiscoveredStrategy,
    SignalListener, LiveSignal, ChannelMessage, ListenerConfig, ListenerState, ExecutionResult,
    CopyExecutor, TelegramChannel,
};
use crate::broker::oanda::OandaClient;
use crate::config::Config;
use crate::db::DbPool;
use crate::models::{Strategy, RiskParams};
use chrono::Utc;
use uuid::Uuid;
use tokio::sync::RwLock;

// Global MTProto client (needs to persist across requests)
lazy_static! {
    static ref MTPROTO_CLIENT: Arc<Mutex<Option<MtprotoClient>>> = Arc::new(Mutex::new(None));
    static ref MTPROTO_GRAMMERS_CLIENT: Arc<Mutex<Option<Arc<Client>>>> = Arc::new(Mutex::new(None));
    static ref SIGNAL_LISTENER: Arc<SignalListener> = Arc::new(SignalListener::new());
    static ref COPY_EXECUTOR: Arc<Mutex<Option<CopyExecutor>>> = Arc::new(Mutex::new(None));
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/copy-trading")
            .route("/status", web::get().to(get_status))
            .route("/test-parse", web::post().to(test_parse))
            .route("/analyze-history", web::post().to(analyze_history))
            .route("/signals", web::get().to(get_signals))
            .route("/config", web::post().to(update_config))
            // MTProto channel scraping
            .route("/mtproto/connect", web::post().to(mtproto_connect))
            .route("/mtproto/reconnect", web::post().to(mtproto_reconnect))
            .route("/mtproto/auth-status", web::get().to(mtproto_auth_status))
            .route("/mtproto/request-code", web::post().to(mtproto_request_code))
            .route("/mtproto/verify-code", web::post().to(mtproto_verify_code))
            .route("/mtproto/list-channels", web::get().to(mtproto_list_channels))
            .route("/mtproto/scrape-channel", web::post().to(mtproto_scrape_channel))
            // AI Strategy Discovery
            .route("/discover-strategy", web::post().to(discover_strategy))
            .route("/save-strategy", web::post().to(save_discovered_strategy))
            // Live Copy Trading
            .route("/live/status", web::get().to(get_live_status))
            .route("/live/start", web::post().to(start_listener))
            .route("/live/stop", web::post().to(stop_listener))
            .route("/live/feed", web::get().to(get_live_feed))
            .route("/live/config", web::get().to(get_listener_config))
            .route("/live/config", web::post().to(update_listener_config))
            .route("/live/execute", web::post().to(execute_signal))
    );
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyTradingStatus {
    pub enabled: bool,
    pub connected: bool,
    pub bot_username: Option<String>,
    pub signals_received: u64,
    pub trades_executed: u64,
    pub auto_execute: bool,
    pub min_confidence: f64,
}

#[derive(Debug, Deserialize)]
pub struct TestParseRequest {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TestParseResponse {
    pub parsed: bool,
    pub signal: Option<TradeSignal>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CopyTradingConfig {
    pub enabled: Option<bool>,
    pub auto_execute: Option<bool>,
    pub min_confidence: Option<f64>,
    pub lot_size: Option<f64>,
}

/// Get current copy trading status
async fn get_status(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    // For now, return a basic status
    // In production, this would check the actual bot connection state
    let status = CopyTradingStatus {
        enabled: config.telegram_bot_token.is_some(),
        connected: false, // Would check actual connection
        bot_username: Some("Au_strat_bot".to_string()),
        signals_received: 0,
        trades_executed: 0,
        auto_execute: false,
        min_confidence: 0.7,
    };

    Ok(HttpResponse::Ok().json(status))
}

/// Test the signal parser without executing
async fn test_parse(
    body: web::Json<TestParseRequest>,
) -> Result<HttpResponse, AppError> {
    let parser = SignalParser::new();
    
    match parser.parse(&body.message) {
        Some(signal) => {
            Ok(HttpResponse::Ok().json(TestParseResponse {
                parsed: true,
                signal: Some(signal),
                error: None,
            }))
        }
        None => {
            Ok(HttpResponse::Ok().json(TestParseResponse {
                parsed: false,
                signal: None,
                error: Some("Could not parse message as a trade signal".to_string()),
            }))
        }
    }
}

/// Get recent signals received
async fn get_signals() -> Result<HttpResponse, AppError> {
    // TODO: Fetch from database
    let signals: Vec<TradeSignal> = vec![];
    Ok(HttpResponse::Ok().json(signals))
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeHistoryRequest {
    pub messages: String,  // Raw text with multiple signals, one per block
}

#[derive(Debug, Serialize)]
pub struct AnalyzeHistoryResponse {
    pub signals_found: usize,
    pub signals: Vec<TradeSignal>,
    pub analysis: SignalAnalysis,
}

/// Analyze bulk historical signals
async fn analyze_history(
    body: web::Json<AnalyzeHistoryRequest>,
) -> Result<HttpResponse, AppError> {
    let monitor = ChannelMonitor::new();
    let parser = SignalParser::new();
    
    // Split by double newlines or "---" to separate signals
    let blocks: Vec<&str> = body.messages
        .split("\n\n\n")
        .flat_map(|s| s.split("---"))
        .filter(|s| !s.trim().is_empty())
        .collect();
    
    // Parse each block as a potential signal
    let mut signals = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if let Some(signal) = parser.parse(block.trim()) {
            signals.push(signal);
        }
    }
    
    // Convert to historical signals for analysis (mock timestamps)
    let historical: Vec<_> = signals.iter().enumerate().map(|(i, s)| {
        crate::telegram::HistoricalSignal {
            signal: s.clone(),
            timestamp: 0,
            message_id: i as i64,
            channel_username: "imported".to_string(),
        }
    }).collect();
    
    let analysis = monitor.analyze_signals(&historical);
    
    Ok(HttpResponse::Ok().json(AnalyzeHistoryResponse {
        signals_found: signals.len(),
        signals,
        analysis,
    }))
}

/// Update copy trading configuration
async fn update_config(
    body: web::Json<CopyTradingConfig>,
) -> Result<HttpResponse, AppError> {
    // TODO: Store config in database
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Configuration updated"
    })))
}

// ============ MTProto Channel Scraping ============

#[derive(Debug, Serialize)]
pub struct MtprotoStatusResponse {
    pub connected: bool,
    pub authorized: bool,
    pub auth_state: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestCodeRequest {
    pub phone: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct ScrapeChannelRequest {
    pub channel_username: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ScrapeChannelResponse {
    pub success: bool,
    pub signals_found: usize,
    pub signals: Vec<HistoricalSignal>,
    pub analysis: SignalAnalysis,
}

/// Connect to Telegram MTProto
async fn mtproto_connect(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    let api_id = config.telegram_api_id
        .ok_or_else(|| AppError::BadRequest("TELEGRAM_API_ID not configured".to_string()))?;
    let api_hash = config.telegram_api_hash.clone()
        .ok_or_else(|| AppError::BadRequest("TELEGRAM_API_HASH not configured".to_string()))?;
    
    let mut client_guard = MTPROTO_CLIENT.lock().await;
    
    // Create new client if needed
    if client_guard.is_none() {
        *client_guard = Some(MtprotoClient::new(api_id, api_hash));
    }
    
    let client = client_guard.as_mut().unwrap();
    
    match client.connect().await {
        Ok(auth_state) => {
            let authorized = matches!(auth_state, AuthState::Authorized);
            let state_str = match &auth_state {
                AuthState::NeedPhone => "need_phone",
                AuthState::NeedCode { .. } => "need_code",
                AuthState::NeedPassword { .. } => "need_password",
                AuthState::Authorized => "authorized",
            };
            
            // If already authorized (from saved session), store the grammers client
            if authorized {
                if let Some(grammers) = client.get_client() {
                    let mut grammers_guard = MTPROTO_GRAMMERS_CLIENT.lock().await;
                    *grammers_guard = Some(Arc::new(grammers.clone()));
                    tracing::info!("Stored grammers client from existing session");
                }
            }
            
            Ok(HttpResponse::Ok().json(MtprotoStatusResponse {
                connected: true,
                authorized,
                auth_state: state_str.to_string(),
            }))
        }
        Err(e) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "connected": false,
                "authorized": false,
                "error": e
            })))
        }
    }
}

/// Reconnect to Telegram (re-use existing session if available)
async fn mtproto_reconnect(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    tracing::info!("Attempting to reconnect Telegram session...");
    
    let api_id = config.telegram_api_id
        .ok_or_else(|| AppError::BadRequest("TELEGRAM_API_ID not configured".to_string()))?;
    let api_hash = config.telegram_api_hash.clone()
        .ok_or_else(|| AppError::BadRequest("TELEGRAM_API_HASH not configured".to_string()))?;
    
    let mut client_guard = MTPROTO_CLIENT.lock().await;
    
    // Always create fresh client on reconnect
    *client_guard = Some(MtprotoClient::new(api_id, api_hash));
    let client = client_guard.as_mut().unwrap();
    
    match client.connect().await {
        Ok(auth_state) => {
            let authorized = matches!(auth_state, AuthState::Authorized);
            let state_str = match &auth_state {
                AuthState::NeedPhone => "need_phone",
                AuthState::NeedCode { .. } => "need_code",
                AuthState::NeedPassword { .. } => "need_password",
                AuthState::Authorized => "authorized",
            };
            
            if authorized {
                if let Some(grammers) = client.get_client() {
                    let mut grammers_guard = MTPROTO_GRAMMERS_CLIENT.lock().await;
                    *grammers_guard = Some(Arc::new(grammers.clone()));
                    tracing::info!("✅ Telegram reconnected successfully");
                }
            }
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": authorized,
                "authorized": authorized,
                "auth_state": state_str,
                "message": if authorized {
                    "Reconnected to Telegram successfully"
                } else {
                    "Re-authentication required"
                }
            })))
        }
        Err(e) => {
            tracing::error!("Failed to reconnect: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "authorized": false,
                "error": e
            })))
        }
    }
}

/// Get current Telegram authentication status
async fn mtproto_auth_status() -> Result<HttpResponse, AppError> {
    let client_guard = MTPROTO_CLIENT.lock().await;
    
    if let Some(client) = client_guard.as_ref() {
        let authorized = client.is_authorized().await;
        
        Ok(HttpResponse::Ok().json(serde_json::json!({
            "connected": true,
            "authorized": authorized,
            "state": if authorized { "authorized" } else { "not_authorized" },
            "message": if authorized {
                "Telegram connection active"
            } else {
                "Telegram authentication required - use /mtproto/connect"
            }
        })))
    } else {
        Ok(HttpResponse::Ok().json(serde_json::json!({
            "connected": false,
            "authorized": false,
            "state": "not_connected",
            "message": "Not connected - use /mtproto/connect first"
        })))
    }
}

/// Request login code
async fn mtproto_request_code(
    body: web::Json<RequestCodeRequest>,
) -> Result<HttpResponse, AppError> {
    let mut client_guard = MTPROTO_CLIENT.lock().await;
    let client = client_guard.as_mut()
        .ok_or_else(|| AppError::BadRequest("Not connected. Call /mtproto/connect first".to_string()))?;
    
    match client.request_login_code(&body.phone).await {
        Ok(_) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "Code sent to your Telegram app",
                "auth_state": "need_code"
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

/// Verify login code
async fn mtproto_verify_code(
    body: web::Json<VerifyCodeRequest>,
) -> Result<HttpResponse, AppError> {
    let mut client_guard = MTPROTO_CLIENT.lock().await;
    let client = client_guard.as_mut()
        .ok_or_else(|| AppError::BadRequest("Not connected".to_string()))?;
    
    match client.sign_in(&body.code).await {
        Ok(auth_state) => {
            let authorized = matches!(auth_state, AuthState::Authorized);
            let state_str = match &auth_state {
                AuthState::Authorized => "authorized",
                AuthState::NeedPassword { .. } => "need_password",
                _ => "unknown",
            };
            
            // Store the grammers client for the signal listener
            if authorized {
                if let Some(grammers) = client.get_client() {
                    let mut grammers_guard = MTPROTO_GRAMMERS_CLIENT.lock().await;
                    *grammers_guard = Some(Arc::new(grammers.clone()));
                    tracing::info!("Stored grammers client for signal listener");
                }
            }
            
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": authorized,
                "auth_state": state_str,
                "message": if authorized { "Successfully authenticated!" } else { "Additional verification needed" }
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

/// List available Telegram channels/groups
async fn mtproto_list_channels() -> Result<HttpResponse, AppError> {
    let client_guard = MTPROTO_CLIENT.lock().await;
    let client = client_guard.as_ref()
        .ok_or_else(|| AppError::BadRequest("Not connected".to_string()))?;
    
    if !client.is_authorized().await {
        return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "success": false,
            "error": "Not authorized. Complete authentication first."
        })));
    }
    
    match client.list_channels().await {
        Ok(channels) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "channels": channels
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

/// Scrape channel messages
async fn mtproto_scrape_channel(
    body: web::Json<ScrapeChannelRequest>,
) -> Result<HttpResponse, AppError> {
    let client_guard = MTPROTO_CLIENT.lock().await;
    let client = client_guard.as_ref()
        .ok_or_else(|| AppError::BadRequest("Not connected".to_string()))?;
    
    if !client.is_authorized().await {
        return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
            "success": false,
            "error": "Not authorized. Complete authentication first."
        })));
    }
    
    let limit = body.limit.unwrap_or(1000);
    let channel = body.channel_username.trim_start_matches('@');
    
    match client.get_channel_messages(channel, limit).await {
        Ok(signals) => {
            let monitor = ChannelMonitor::new();
            let analysis = monitor.analyze_signals(&signals);
            
            Ok(HttpResponse::Ok().json(ScrapeChannelResponse {
                success: true,
                signals_found: signals.len(),
                signals,
                analysis,
            }))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

// ============ AI Strategy Discovery ============

#[derive(Debug, Deserialize)]
pub struct DiscoverStrategyRequest {
    pub signals: Vec<TradeSignal>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverStrategyResponse {
    pub success: bool,
    pub strategy: Option<DiscoveredStrategy>,
    pub error: Option<String>,
}

/// Use AI to discover trading patterns from signals
async fn discover_strategy(
    config: web::Data<Config>,
    body: web::Json<DiscoverStrategyRequest>,
) -> Result<HttpResponse, AppError> {
    let api_key = config.openai_api_key.clone()
        .ok_or_else(|| AppError::BadRequest("OPENAI_API_KEY not configured".to_string()))?;
    
    if body.signals.is_empty() {
        return Ok(HttpResponse::BadRequest().json(DiscoverStrategyResponse {
            success: false,
            strategy: None,
            error: Some("No signals provided for analysis".to_string()),
        }));
    }
    
    let discovery = StrategyDiscovery::new(api_key);
    
    match discovery.discover_strategy(&body.signals).await {
        Ok(strategy) => {
            Ok(HttpResponse::Ok().json(DiscoverStrategyResponse {
                success: true,
                strategy: Some(strategy),
                error: None,
            }))
        }
        Err(e) => {
            Ok(HttpResponse::Ok().json(DiscoverStrategyResponse {
                success: false,
                strategy: None,
                error: Some(e),
            }))
        }
    }
}

// ============ Save Discovered Strategy ============

#[derive(Debug, Deserialize)]
pub struct SaveStrategyRequest {
    pub strategy: DiscoveredStrategy,
    pub source_channel: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SaveStrategyResponse {
    pub success: bool,
    pub strategy_id: Option<Uuid>,
    pub error: Option<String>,
}

/// Save a discovered strategy to the database
async fn save_discovered_strategy(
    pool: web::Data<DbPool>,
    body: web::Json<SaveStrategyRequest>,
) -> Result<HttpResponse, AppError> {
    let discovered = &body.strategy;
    
    // Convert discovered strategy rules to custom_rules JSON format
    let custom_rules = serde_json::json!({
        "entry_long": discovered.entry_rules.iter()
            .filter(|r| r.description.to_lowercase().contains("buy") || !r.description.to_lowercase().contains("sell"))
            .map(|r| serde_json::json!({
                "indicator": r.indicator,
                "condition": r.condition,
                "value": r.value,
                "description": r.description
            }))
            .collect::<Vec<_>>(),
        "entry_short": discovered.entry_rules.iter()
            .filter(|r| r.description.to_lowercase().contains("sell") || r.description.to_lowercase().contains("short"))
            .map(|r| serde_json::json!({
                "indicator": r.indicator,
                "condition": r.condition,
                "value": r.value,
                "description": r.description
            }))
            .collect::<Vec<_>>(),
        "exit": discovered.exit_rules.iter()
            .map(|r| serde_json::json!({
                "indicator": r.indicator,
                "condition": r.condition,
                "value": r.value,
                "description": r.description
            }))
            .collect::<Vec<_>>(),
        "observations": discovered.observations,
        "ai_confidence": discovered.confidence_score,
        "source_channel": body.source_channel,
    });
    
    // Convert risk management
    let risk = RiskParams {
        stop_loss_pct: discovered.risk_management.suggested_sl_pips / 100.0, // Convert pips to rough %
        take_profit_pct: discovered.risk_management.suggested_tp_pips / 100.0,
        position_size_pct: 2.0, // Default
        max_daily_loss_pct: 5.0, // Default
    };
    
    // Strategy params
    let params = serde_json::json!({
        "sl_pips": discovered.risk_management.suggested_sl_pips,
        "tp_pips": discovered.risk_management.suggested_tp_pips,
        "risk_reward": discovered.risk_management.risk_reward_ratio,
    });
    
    let now = Utc::now();
    let user_id = Uuid::nil(); // TODO: Get from JWT
    
    let result = sqlx::query_as::<_, Strategy>(
        r#"
        INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, custom_rules, is_active, created_at, updated_at)
        VALUES ($1, $2, $3, 'custom', $4, $5, $6, false, $7, $7)
        RETURNING *
        "#
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&discovered.name)
    .bind(&params)
    .bind(serde_json::to_value(&risk).unwrap())
    .bind(&custom_rules)
    .bind(now)
    .fetch_one(pool.get_ref())
    .await;
    
    match result {
        Ok(strategy) => {
            Ok(HttpResponse::Ok().json(SaveStrategyResponse {
                success: true,
                strategy_id: Some(strategy.id),
                error: None,
            }))
        }
        Err(e) => {
            Ok(HttpResponse::Ok().json(SaveStrategyResponse {
                success: false,
                strategy_id: None,
                error: Some(format!("Failed to save strategy: {}", e)),
            }))
        }
    }
}

// ============ Live Copy Trading ============

#[derive(Debug, Serialize)]
pub struct LiveStatusResponse {
    pub listener_state: String,
    pub channel_username: String,
    pub auto_execute: bool,
    pub min_confidence: f64,
    pub lot_size: f64,
    pub max_daily_trades: u32,
    pub remaining_trades: u32,
    pub recent_signals_count: usize,
    pub mtproto_authorized: bool,
}

/// Get live copy trading status
async fn get_live_status(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    let listener = SIGNAL_LISTENER.clone();
    let listener_config = listener.get_config().await;
    let state = listener.get_state().await;
    let recent_signals = listener.get_recent_signals(100).await;
    
    // Check MTProto auth status
    let mtproto_authorized = {
        let client_guard = MTPROTO_CLIENT.lock().await;
        if let Some(ref client) = *client_guard {
            client.is_authorized().await
        } else {
            false
        }
    };
    
    let state_str = match state {
        ListenerState::Stopped => "stopped",
        ListenerState::Starting => "starting",
        ListenerState::Running => "running",
        ListenerState::Error(_) => "error",
    };
    
    Ok(HttpResponse::Ok().json(LiveStatusResponse {
        listener_state: state_str.to_string(),
        channel_username: listener_config.channel_username,
        auto_execute: listener_config.auto_execute,
        min_confidence: listener_config.min_confidence,
        lot_size: listener_config.lot_size,
        max_daily_trades: listener_config.max_daily_trades,
        remaining_trades: listener_config.max_daily_trades, // TODO: Track actual count
        recent_signals_count: recent_signals.len(),
        mtproto_authorized,
    }))
}

#[derive(Debug, Deserialize)]
pub struct StartListenerRequest {
    pub channel_username: Option<String>,
}

/// Start the live signal listener
async fn start_listener(
    config: web::Data<Config>,
    body: web::Json<StartListenerRequest>,
) -> Result<HttpResponse, AppError> {
    // Get or create grammers client from MTProto client
    let grammers_client = {
        let client_guard = MTPROTO_GRAMMERS_CLIENT.lock().await;
        client_guard.clone()
    };
    
    let grammers_client = match grammers_client {
        Some(client) => client,
        None => {
            // Try to get from MTProto connection
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": "Not connected to Telegram. Call /copy-trading/mtproto/connect first and authenticate."
            })));
        }
    };
    
    let listener = SIGNAL_LISTENER.clone();
    
    // Update channel if provided
    if let Some(channel) = &body.channel_username {
        let mut current_config = listener.get_config().await;
        current_config.channel_username = channel.trim_start_matches('@').to_string();
        listener.update_config(current_config).await;
    }
    
    // Create OANDA client for auto-execution
    let oanda = match (&config.oanda_api_token, &config.oanda_account_id) {
        (Some(token), Some(account_id)) => {
            match OandaClient::new(token, account_id, config.oanda_practice) {
                Ok(client) => {
                    tracing::info!("OANDA client created for copy trade auto-execution");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    tracing::warn!("Failed to create OANDA client for auto-execute: {}", e);
                    None
                }
            }
        }
        _ => {
            tracing::warn!("OANDA credentials not configured - auto-execute disabled");
            None
        }
    };
    
    // Start the listener
    match listener.start(grammers_client, oanda).await {
        Ok(_) => {
            let config = listener.get_config().await;
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": format!("Listener started for @{}", config.channel_username),
                "channel": config.channel_username
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

/// Stop the live signal listener
async fn stop_listener() -> Result<HttpResponse, AppError> {
    let listener = SIGNAL_LISTENER.clone();
    
    match listener.stop().await {
        Ok(_) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "Listener stopped"
            })))
        }
        Err(e) => {
            Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": e
            })))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LiveFeedResponse {
    pub messages: Vec<ChannelMessage>,
    pub signals: Vec<LiveSignal>,
}

/// Get live feed (recent messages and signals)
async fn get_live_feed(
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let limit: usize = query.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    
    let listener = SIGNAL_LISTENER.clone();
    let messages = listener.get_recent_messages(limit).await;
    let signals = listener.get_recent_signals(limit).await;
    
    Ok(HttpResponse::Ok().json(LiveFeedResponse {
        messages,
        signals,
    }))
}

/// Get listener configuration
async fn get_listener_config() -> Result<HttpResponse, AppError> {
    let listener = SIGNAL_LISTENER.clone();
    let config = listener.get_config().await;
    Ok(HttpResponse::Ok().json(config))
}

#[derive(Debug, Deserialize)]
pub struct UpdateListenerConfigRequest {
    pub channel_username: Option<String>,
    pub auto_execute: Option<bool>,
    pub min_confidence: Option<f64>,
    pub lot_size: Option<f64>,
    pub max_daily_trades: Option<u32>,
}

/// Update listener configuration
async fn update_listener_config(
    body: web::Json<UpdateListenerConfigRequest>,
) -> Result<HttpResponse, AppError> {
    let listener = SIGNAL_LISTENER.clone();
    let mut config = listener.get_config().await;
    
    if let Some(channel) = &body.channel_username {
        config.channel_username = channel.trim_start_matches('@').to_string();
    }
    if let Some(auto_execute) = body.auto_execute {
        config.auto_execute = auto_execute;
    }
    if let Some(min_confidence) = body.min_confidence {
        config.min_confidence = min_confidence;
    }
    if let Some(lot_size) = body.lot_size {
        config.lot_size = lot_size;
    }
    if let Some(max_daily) = body.max_daily_trades {
        config.max_daily_trades = max_daily;
    }
    
    listener.update_config(config.clone()).await;
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "config": config
    })))
}

#[derive(Debug, Deserialize)]
pub struct ExecuteSignalRequest {
    pub signal_id: String,
    pub lot_size: Option<f64>,
}

/// Manually execute a signal
async fn execute_signal(
    config: web::Data<Config>,
    body: web::Json<ExecuteSignalRequest>,
) -> Result<HttpResponse, AppError> {
    let listener = SIGNAL_LISTENER.clone();
    
    // Find the signal
    let signals = listener.get_recent_signals(100).await;
    let signal = signals.iter().find(|s| s.id == body.signal_id);
    
    let signal = match signal {
        Some(s) => s.clone(),
        None => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "error": "Signal not found"
            })));
        }
    };
    
    if signal.executed {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": "Signal already executed"
        })));
    }
    
    // Create broker client
    let oanda = OandaClient::new(
        &config.oanda_api_token.clone().unwrap_or_default(),
        &config.oanda_account_id.clone().unwrap_or_default(),
        config.oanda_practice,
    ).map_err(|e| AppError::InternalError(format!("Failed to create OANDA client: {}", e)))?;
    
    let listener_config = listener.get_config().await;
    let lot_size = body.lot_size.unwrap_or(listener_config.lot_size);
    
    // Create executor
    let config_arc = Arc::new(RwLock::new(listener_config));
    let executor = CopyExecutor::new(Arc::new(oanda), config_arc);
    
    // Execute the trade
    let result = executor.execute_with_lot_size(&signal, lot_size).await;
    
    // Mark signal as executed
    listener.mark_executed(&signal.id, result.clone()).await;
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": result.success,
        "result": result
    })))
}
