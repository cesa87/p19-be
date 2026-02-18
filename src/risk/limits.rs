//! Risk limits configuration
//!
//! Re-exports RiskLimits from models for convenience

pub use crate::risk::models::RiskLimits;

/// Create default conservative limits
pub fn conservative_limits() -> RiskLimits {
    RiskLimits {
        max_daily_loss_pct: 2.0,   // Tighter
        max_weekly_loss_pct: 4.0,  // Tighter
        max_drawdown_pct: 8.0,     // Tighter
        max_consecutive_losses: 6,
        max_total_exposure_multiplier: 3.0,  // Lower
        max_per_asset_multiplier: 1.5,       // Lower
        max_per_strategy_multiplier: 1.0,
    }
}

/// Create aggressive limits (use with caution!)
pub fn aggressive_limits() -> RiskLimits {
    RiskLimits {
        max_daily_loss_pct: 5.0,
        max_weekly_loss_pct: 10.0,
        max_drawdown_pct: 15.0,
        max_consecutive_losses: 12,
        max_total_exposure_multiplier: 10.0,
        max_per_asset_multiplier: 5.0,
        max_per_strategy_multiplier: 3.0,
    }
}
