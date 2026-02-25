//! Bot activity logging - tracks what bots are doing in real-time

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    Info,
    Warning,
    Signal,
    OrderPlaced,
    OrderFilled,
    OrderClosed,
    StopLoss,
    TakeProfit,
    Error,
    PriceUpdate,
    IntelligenceGateBlock,   // Signal would have been blocked by intelligence gate
    IntelligenceGateReduce,  // Signal would have had size reduced
    IntelligenceGateBoost,   // Signal would have had size boosted
    IntelligenceGateAllow,   // Signal passed through gate unchanged
}

impl ToString for ActivityType {
    fn to_string(&self) -> String {
        match self {
            ActivityType::Info => "info".to_string(),
            ActivityType::Warning => "warning".to_string(),
            ActivityType::Signal => "signal".to_string(),
            ActivityType::OrderPlaced => "order_placed".to_string(),
            ActivityType::OrderFilled => "order_filled".to_string(),
            ActivityType::OrderClosed => "order_closed".to_string(),
            ActivityType::StopLoss => "stop_loss".to_string(),
            ActivityType::TakeProfit => "take_profit".to_string(),
            ActivityType::Error => "error".to_string(),
            ActivityType::PriceUpdate => "price_update".to_string(),
            ActivityType::IntelligenceGateBlock => "intelligence_gate_block".to_string(),
            ActivityType::IntelligenceGateReduce => "intelligence_gate_reduce".to_string(),
            ActivityType::IntelligenceGateBoost => "intelligence_gate_boost".to_string(),
            ActivityType::IntelligenceGateAllow => "intelligence_gate_allow".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BotActivity {
    pub id: Uuid,
    pub bot_id: Uuid,
    pub activity_type: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotActivityResponse {
    pub id: String,
    pub bot_id: String,
    pub activity_type: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub created_at: String,
}

impl From<BotActivity> for BotActivityResponse {
    fn from(a: BotActivity) -> Self {
        Self {
            id: a.id.to_string(),
            bot_id: a.bot_id.to_string(),
            activity_type: a.activity_type,
            message: a.message,
            details: a.details,
            created_at: a.created_at.to_rfc3339(),
        }
    }
}

/// Log a bot activity to the database
pub async fn log_activity(
    pool: &sqlx::PgPool,
    bot_id: Uuid,
    activity_type: ActivityType,
    message: &str,
    details: Option<serde_json::Value>,
) -> Result<BotActivity, sqlx::Error> {
    let activity = sqlx::query_as::<_, BotActivity>(
        r#"
        INSERT INTO bot_activities (bot_id, activity_type, message, details)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#
    )
    .bind(bot_id)
    .bind(activity_type.to_string())
    .bind(message)
    .bind(details)
    .fetch_one(pool)
    .await?;
    
    Ok(activity)
}

/// Get recent activities for a bot
pub async fn get_recent_activities(
    pool: &sqlx::PgPool,
    bot_id: Uuid,
    limit: i64,
) -> Result<Vec<BotActivity>, sqlx::Error> {
    let activities = sqlx::query_as::<_, BotActivity>(
        r#"
        SELECT * FROM bot_activities 
        WHERE bot_id = $1 
        ORDER BY created_at DESC 
        LIMIT $2
        "#
    )
    .bind(bot_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    Ok(activities)
}

/// Get recent activities for all active bots
pub async fn get_all_active_activities(
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<Vec<BotActivity>, sqlx::Error> {
    let activities = sqlx::query_as::<_, BotActivity>(
        r#"
        SELECT ba.* FROM bot_activities ba
        JOIN bots b ON ba.bot_id = b.id
        WHERE b.is_active = true
        ORDER BY ba.created_at DESC 
        LIMIT $1
        "#
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    Ok(activities)
}
