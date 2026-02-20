//! MRATE Global Settings
//! 
//! Controls the global MRATE on/off switch.
//! When MRATE is disabled, bots trade freely without any MRATE filtering,
//! direction blocking, lot size adjustments, or orchestrator auto-enable/disable.

use sqlx::PgPool;

/// Check if MRATE is globally enabled.
/// Returns false if the setting doesn't exist or on DB error (fail-open = bots trade freely).
pub async fn is_mrate_enabled(pool: &PgPool) -> bool {
    let result: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_settings WHERE key = 'mrate_enabled'"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    matches!(result.as_deref(), Some("true"))
}

/// Set the MRATE enabled/disabled state.
pub async fn set_mrate_enabled(pool: &PgPool, enabled: bool) -> Result<(), sqlx::Error> {
    let value = if enabled { "true" } else { "false" };
    
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ('mrate_enabled', $1, NOW())
         ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
    )
    .bind(value)
    .execute(pool)
    .await?;

    tracing::info!(
        "{}  MRATE globally {}",
        if enabled { "✅" } else { "🛑" },
        if enabled { "ENABLED" } else { "DISABLED — bots trade freely" }
    );

    Ok(())
}
