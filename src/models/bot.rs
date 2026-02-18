use chrono::{DateTime, Utc, NaiveTime};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Note: DB uses DECIMAL but we use f64 in Rust. Postgres handles the conversion.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Bot {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub strategy_id: Option<Uuid>,
    pub broker: String,
    pub account_type: String,
    pub api_token: Option<String>,
    pub account_id: Option<String>,
    pub lot_size: f64,
    pub max_positions: i32,
    pub max_daily_trades: Option<i32>,
    pub max_daily_loss_pct: Option<f64>,
    pub use_strategy_sl_tp: Option<bool>,
    pub override_sl_pips: Option<f64>,
    pub override_tp_pips: Option<f64>,
    pub trading_hours_start: Option<NaiveTime>,
    pub trading_hours_end: Option<NaiveTime>,
    pub trading_days: Option<String>,
    pub is_active: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_stopped_at: Option<DateTime<Utc>>,
    pub total_trades: Option<i32>,
    pub total_pnl: Option<f64>,
    pub cooldown_seconds: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub strategy_id: Option<Uuid>,
    pub broker: Option<String>,
    pub account_type: String,  // "demo" or "live"
    pub api_token: Option<String>,
    pub account_id: Option<String>,
    pub lot_size: Option<f64>,
    pub max_positions: Option<i32>,
    pub max_daily_trades: Option<i32>,
    pub max_daily_loss_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBotRequest {
    pub name: Option<String>,
    pub strategy_id: Option<Uuid>,
    pub account_type: Option<String>,
    pub api_token: Option<String>,
    pub account_id: Option<String>,
    pub lot_size: Option<f64>,
    pub max_positions: Option<i32>,
    pub max_daily_trades: Option<i32>,
    pub max_daily_loss_pct: Option<f64>,
    pub use_strategy_sl_tp: Option<bool>,
    pub override_sl_pips: Option<f64>,
    pub override_tp_pips: Option<f64>,
    pub trading_hours_start: Option<String>,
    pub trading_hours_end: Option<String>,
    pub trading_days: Option<String>,
    pub cooldown_seconds: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotResponse {
    pub id: Uuid,
    pub name: String,
    pub strategy_id: Option<Uuid>,
    pub strategy_name: Option<String>,
    pub timeframe: Option<String>,
    pub broker: String,
    pub account_type: String,
    pub account_id: Option<String>,
    pub lot_size: f64,
    pub max_positions: i32,
    pub max_daily_trades: Option<i32>,
    pub max_daily_loss_pct: Option<f64>,
    pub use_strategy_sl_tp: bool,
    pub override_sl_pips: Option<f64>,
    pub override_tp_pips: Option<f64>,
    pub trading_hours_start: Option<String>,
    pub trading_hours_end: Option<String>,
    pub trading_days: Option<String>,
    pub cooldown_seconds: Option<i32>,
    pub is_active: bool,
    pub total_trades: i32,
    pub total_pnl: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Bot> for BotResponse {
    fn from(b: Bot) -> Self {
        Self {
            id: b.id,
            name: b.name,
            strategy_id: b.strategy_id,
            strategy_name: None, // Filled in by API
            timeframe: None, // Filled in by API from strategy params
            broker: b.broker,
            account_type: b.account_type,
            account_id: b.account_id,
            lot_size: b.lot_size,
            max_positions: b.max_positions,
            max_daily_trades: b.max_daily_trades,
            max_daily_loss_pct: b.max_daily_loss_pct,
            use_strategy_sl_tp: b.use_strategy_sl_tp.unwrap_or(true),
            override_sl_pips: b.override_sl_pips,
            override_tp_pips: b.override_tp_pips,
            trading_hours_start: b.trading_hours_start.map(|t| t.to_string()),
            trading_hours_end: b.trading_hours_end.map(|t| t.to_string()),
            trading_days: b.trading_days,
            cooldown_seconds: b.cooldown_seconds,
            is_active: b.is_active,
            total_trades: b.total_trades.unwrap_or(0),
            total_pnl: b.total_pnl.unwrap_or(0.0),
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}
