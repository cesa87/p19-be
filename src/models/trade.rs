use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeStatus {
    Open,
    Closed,
    Pending,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Trade {
    pub id: Uuid,
    pub strategy_id: Uuid,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub pnl: Option<f64>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    pub symbol: String,
    pub direction: TradeDirection,
    pub entry_price: f64,
    pub current_price: f64,
    pub quantity: f64,
    pub unrealized_pnl: f64,
    pub opened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResponse {
    pub id: Uuid,
    pub strategy_id: Uuid,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub quantity: f64,
    pub pnl: Option<f64>,
    pub entry_time: DateTime<Utc>,
    pub exit_time: Option<DateTime<Utc>>,
    pub status: String,
}

impl From<Trade> for TradeResponse {
    fn from(t: Trade) -> Self {
        Self {
            id: t.id,
            strategy_id: t.strategy_id,
            symbol: t.symbol,
            direction: t.direction,
            entry_price: t.entry_price,
            exit_price: t.exit_price,
            quantity: t.quantity,
            pnl: t.pnl,
            entry_time: t.entry_time,
            exit_time: t.exit_time,
            status: t.status,
        }
    }
}

// Bot state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum BotStatus {
    Running,
    Stopped,
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotState {
    pub status: BotStatus,
    pub active_strategy_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub today_pnl: f64,
    pub open_positions: usize,
    pub error_message: Option<String>,
}

impl Default for BotState {
    fn default() -> Self {
        Self {
            status: BotStatus::Stopped,
            active_strategy_id: None,
            started_at: None,
            today_pnl: 0.0,
            open_positions: 0,
            error_message: None,
        }
    }
}
