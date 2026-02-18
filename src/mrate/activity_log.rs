//! MRATE Activity Logger
//! 
//! Records what the MRATE engine is doing/seeing for UI transparency

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, FromRow};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MrateActivity {
    pub id: Uuid,
    pub timestamp: NaiveDateTime,
    pub activity_type: String,
    pub source: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub severity: Option<String>,
}

/// Activity types for categorization
pub enum ActivityType {
    FeedFetch,      // Successfully fetched data from a feed
    FeedError,      // Failed to fetch from a feed
    RegimeChange,   // Regime transitioned
    ScoreUpdate,    // Scores recalculated
    DataWarning,    // Data quality issue
    Decision,       // Trading decision made
}

impl ActivityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActivityType::FeedFetch => "feed_fetch",
            ActivityType::FeedError => "feed_error",
            ActivityType::RegimeChange => "regime_change",
            ActivityType::ScoreUpdate => "score_update",
            ActivityType::DataWarning => "data_warning",
            ActivityType::Decision => "decision",
        }
    }
}

/// Log an MRATE activity to the database
pub async fn log_mrate_activity(
    pool: &PgPool,
    activity_type: ActivityType,
    source: Option<&str>,
    message: &str,
    details: Option<serde_json::Value>,
    severity: &str,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO mrate_activities (id, timestamp, activity_type, source, message, details, severity)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#
    )
    .bind(Uuid::new_v4())
    .bind(Utc::now())
    .bind(activity_type.as_str())
    .bind(source)
    .bind(message)
    .bind(details)
    .bind(severity)
    .execute(pool)
    .await;
    
    if let Err(e) = result {
        tracing::warn!("Failed to log MRATE activity: {}", e);
    }
}

/// Convenience functions for common log types
pub async fn log_feed_success(pool: &PgPool, source: &str, message: &str, details: Option<serde_json::Value>) {
    log_mrate_activity(pool, ActivityType::FeedFetch, Some(source), message, details, "success").await;
}

pub async fn log_feed_error(pool: &PgPool, source: &str, message: &str) {
    log_mrate_activity(pool, ActivityType::FeedError, Some(source), message, None, "error").await;
}

pub async fn log_regime_change(pool: &PgPool, from: &str, to: &str, reason: &str, details: Option<serde_json::Value>) {
    let message = format!("Regime changed: {} → {} | {}", from, to, reason);
    log_mrate_activity(pool, ActivityType::RegimeChange, None, &message, details, "warning").await;
}

pub async fn log_score_update(pool: &PgPool, message: &str, details: Option<serde_json::Value>) {
    log_mrate_activity(pool, ActivityType::ScoreUpdate, None, message, details, "info").await;
}

pub async fn log_data_warning(pool: &PgPool, source: &str, message: &str) {
    log_mrate_activity(pool, ActivityType::DataWarning, Some(source), message, None, "warning").await;
}

pub async fn log_decision(pool: &PgPool, message: &str, details: Option<serde_json::Value>) {
    log_mrate_activity(pool, ActivityType::Decision, None, message, details, "info").await;
}

/// Get recent MRATE activities for the UI
pub async fn get_recent_activities(pool: &PgPool, limit: i32) -> Result<Vec<MrateActivity>, sqlx::Error> {
    let activities = sqlx::query_as::<_, MrateActivity>(
        r#"
        SELECT id, timestamp, activity_type, source, message, details, severity
        FROM mrate_activities
        ORDER BY timestamp DESC
        LIMIT $1
        "#
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    
    Ok(activities)
}

/// Get activities filtered by type
pub async fn get_activities_by_type(pool: &PgPool, activity_type: &str, limit: i32) -> Result<Vec<MrateActivity>, sqlx::Error> {
    let activities = sqlx::query_as::<_, MrateActivity>(
        r#"
        SELECT id, timestamp, activity_type, source, message, details, severity
        FROM mrate_activities
        WHERE activity_type = $1
        ORDER BY timestamp DESC
        LIMIT $2
        "#
    )
    .bind(activity_type)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    
    Ok(activities)
}
