//! Intelligence Gate Settings
//!
//! Per-bot intelligence gate toggle and Polymarket USDC trading limits.
//! All stored in app_settings (key/value) for live editability.

use sqlx::PgPool;
use uuid::Uuid;

// ─── Per-bot gate toggle ──────────────────────────────────────────────────────

/// Check if intelligence gating is enabled for a specific bot.
/// Returns false on error (fail-open = no gating).
pub async fn get_bot_intelligence_gate(pool: &PgPool, bot_id: Uuid) -> bool {
    let result: Option<bool> = sqlx::query_scalar(
        "SELECT intelligence_gate_enabled FROM bots WHERE id = $1"
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    result.unwrap_or(false)
}

/// Enable or disable intelligence gating for a specific bot.
pub async fn set_bot_intelligence_gate(pool: &PgPool, bot_id: Uuid, enabled: bool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE bots SET intelligence_gate_enabled = $1, updated_at = NOW() WHERE id = $2"
    )
    .bind(enabled)
    .bind(bot_id)
    .execute(pool)
    .await?;

    tracing::info!(
        "🧠 Intelligence gate {} for bot {}",
        if enabled { "ENABLED" } else { "DISABLED (observe-only)" },
        bot_id
    );

    Ok(())
}

// ─── Gate thresholds (global) ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateThresholds {
    /// Tension score above this = BLOCK trade (0-100)
    pub block_tension: f64,
    /// Tension score above this = REDUCE size by 50% (0-100)  
    pub reduce_tension: f64,
    /// Opportunity score above this + aligned direction = BOOST size 1.25x (0-100)
    pub boost_opportunity: f64,
    /// Minimum confidence to apply gating at all (0-1)
    pub min_confidence: f64,
}

impl Default for GateThresholds {
    fn default() -> Self {
        Self {
            block_tension: 75.0,
            reduce_tension: 60.0,
            boost_opportunity: 70.0,
            min_confidence: 0.3,
        }
    }
}

async fn get_f64_setting(pool: &PgPool, key: &str, default: f64) -> f64 {
    let result: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_settings WHERE key = $1"
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    result.and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub async fn get_gate_thresholds(pool: &PgPool) -> GateThresholds {
    GateThresholds {
        block_tension: get_f64_setting(pool, "intel_gate_block_tension", 75.0).await,
        reduce_tension: get_f64_setting(pool, "intel_gate_reduce_tension", 60.0).await,
        boost_opportunity: get_f64_setting(pool, "intel_gate_boost_opportunity", 70.0).await,
        min_confidence: get_f64_setting(pool, "intel_gate_min_confidence", 0.3).await,
    }
}

pub async fn set_gate_threshold(pool: &PgPool, key: &str, value: f64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()"
    )
    .bind(key)
    .bind(value.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

// ─── Polymarket USDC trading limits ──────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolymarketSettings {
    pub enabled: bool,
    pub max_trade_usdc: f64,
    pub max_total_usdc: f64,
}

pub async fn get_polymarket_settings(pool: &PgPool) -> PolymarketSettings {
    let enabled = {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_settings WHERE key = 'polymarket_trading_enabled'"
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        matches!(result.as_deref(), Some("true"))
    };

    PolymarketSettings {
        enabled,
        max_trade_usdc: get_f64_setting(pool, "polymarket_max_trade_usdc", 50.0).await,
        max_total_usdc: get_f64_setting(pool, "polymarket_max_total_usdc", 5000.0).await,
    }
}

pub async fn set_polymarket_setting(pool: &PgPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()"
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;

    tracing::info!("💰 Polymarket setting updated: {} = {}", key, value);
    Ok(())
}
