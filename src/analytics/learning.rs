//! Learning Module - Adaptive multipliers from trade history
//!
//! Queries trade_context to calculate how well a bot has performed under
//! specific conditions (regime, session, volatility) and returns multipliers
//! that adjust signal confidence and position sizing.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Minimum closed trades required before learning adjustments kick in
const MIN_TRADES_FOR_LEARNING: i64 = 8;

/// Result of the learning query — multipliers to apply to trading decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningMultiplier {
    /// Scales signal confidence (0.7 to 1.3)
    pub confidence_mult: f64,
    /// Scales position size (0.5 to 1.5)
    pub lot_mult: f64,
    /// Number of trades analyzed
    pub trades_analyzed: i64,
    /// Historical win rate for these conditions
    pub win_rate: f64,
    /// Historical profit factor for these conditions
    pub profit_factor: f64,
    /// Average P&L per trade
    pub avg_pnl: f64,
    /// Whether we have enough data to make adjustments
    pub has_sufficient_data: bool,
    /// Human-readable summary
    pub summary: String,
}

impl LearningMultiplier {
    /// Neutral multiplier (no adjustment) — used during cold start
    pub fn neutral() -> Self {
        Self {
            confidence_mult: 1.0,
            lot_mult: 1.0,
            trades_analyzed: 0,
            win_rate: 0.0,
            profit_factor: 0.0,
            avg_pnl: 0.0,
            has_sufficient_data: false,
            summary: "Cold start — no historical data yet".to_string(),
        }
    }
}

/// Raw performance data from trade_context query
#[derive(Debug, sqlx::FromRow)]
struct PerformanceRow {
    total_trades: i64,
    winners: i64,
    losers: i64,
    total_pnl: Option<f64>,
    gross_profit: Option<f64>,
    gross_loss: Option<f64>,
}

/// Calculate learning multipliers for a bot under current market conditions.
///
/// Queries trade_context for closed trades matching the given regime + session,
/// then maps win rate → confidence multiplier and profit factor → lot multiplier.
pub async fn calculate_learning_multiplier(
    pool: &PgPool,
    bot_id: Uuid,
    regime: &str,
    session: &str,
) -> LearningMultiplier {
    // Query historical performance for this bot in current regime + session
    let result = sqlx::query_as::<_, PerformanceRow>(
        r#"
        SELECT 
            COUNT(*) as total_trades,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl,
            SUM(pnl) FILTER (WHERE pnl > 0)::FLOAT8 as gross_profit,
            SUM(ABS(pnl)) FILTER (WHERE pnl < 0)::FLOAT8 as gross_loss
        FROM trade_context
        WHERE bot_id = $1 
          AND mrate_regime = $2
          AND trading_session = $3
          AND is_winner IS NOT NULL
        "#,
    )
    .bind(bot_id)
    .bind(regime)
    .bind(session)
    .fetch_one(pool)
    .await;

    let row = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Learning query failed for bot {}: {}", bot_id, e);
            return LearningMultiplier::neutral();
        }
    };

    // Cold start protection
    if row.total_trades < MIN_TRADES_FOR_LEARNING {
        return LearningMultiplier {
            trades_analyzed: row.total_trades,
            summary: format!(
                "Insufficient data: {}/{} trades in {} + {}",
                row.total_trades, MIN_TRADES_FOR_LEARNING, regime, session
            ),
            ..LearningMultiplier::neutral()
        };
    }

    let win_rate = row.winners as f64 / row.total_trades as f64;
    let avg_pnl = row.total_pnl.unwrap_or(0.0) / row.total_trades as f64;
    
    let gross_profit = row.gross_profit.unwrap_or(0.0);
    let gross_loss = row.gross_loss.unwrap_or(0.0);
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        10.0 // Cap at 10 for perfect record
    } else {
        0.0
    };

    // Map win rate → confidence multiplier
    let confidence_mult = if win_rate < 0.30 {
        0.7   // Historically terrible — reduce confidence significantly
    } else if win_rate < 0.40 {
        0.80
    } else if win_rate < 0.45 {
        0.90
    } else if win_rate < 0.55 {
        1.0   // Neutral zone
    } else if win_rate < 0.65 {
        1.1   // Historically good
    } else {
        1.3   // Historically excellent
    };

    // Map profit factor → lot size multiplier
    let lot_mult = if profit_factor < 0.6 {
        0.5   // Losing money badly — cut size in half
    } else if profit_factor < 0.8 {
        0.7
    } else if profit_factor < 1.0 {
        0.85  // Slight loss — reduce a bit
    } else if profit_factor < 1.5 {
        1.0   // Breakeven to modest profit — normal size
    } else if profit_factor < 2.0 {
        1.2   // Good profit factor — size up slightly
    } else {
        1.5   // Excellent — max size up
    };

    let summary = format!(
        "{} trades in {}+{}: {:.0}% win rate, PF={:.2}, avg=${:.2} → conf={:.2}x, lot={:.2}x",
        row.total_trades, regime, session,
        win_rate * 100.0, profit_factor, avg_pnl,
        confidence_mult, lot_mult
    );

    LearningMultiplier {
        confidence_mult,
        lot_mult,
        trades_analyzed: row.total_trades,
        win_rate,
        profit_factor,
        avg_pnl,
        has_sufficient_data: true,
        summary,
    }
}

/// Query historical performance for the orchestrator (regime + session).
/// Returns (win_rate, trade_count, avg_pnl) or None if insufficient data.
pub async fn query_historical_performance(
    pool: &PgPool,
    bot_id: Uuid,
    regime: &str,
    session: &str,
) -> Option<(f64, i64, f64)> {
    let row = sqlx::query_as::<_, PerformanceRow>(
        r#"
        SELECT 
            COUNT(*) as total_trades,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl,
            SUM(pnl) FILTER (WHERE pnl > 0)::FLOAT8 as gross_profit,
            SUM(ABS(pnl)) FILTER (WHERE pnl < 0)::FLOAT8 as gross_loss
        FROM trade_context
        WHERE bot_id = $1 
          AND mrate_regime = $2
          AND trading_session = $3
          AND is_winner IS NOT NULL
        "#,
    )
    .bind(bot_id)
    .bind(regime)
    .bind(session)
    .fetch_one(pool)
    .await
    .ok()?;

    if row.total_trades < MIN_TRADES_FOR_LEARNING {
        return None;
    }

    let win_rate = row.winners as f64 / row.total_trades as f64;
    let avg_pnl = row.total_pnl.unwrap_or(0.0) / row.total_trades as f64;

    Some((win_rate, row.total_trades, avg_pnl))
}

/// Calculate the orchestrator score adjustment based on historical performance.
/// Returns (score_adjustment, reason_string).
pub fn performance_score_adjustment(win_rate: f64, trade_count: i64, avg_pnl: f64) -> (f64, String) {
    let adjustment = if win_rate > 0.60 {
        15.0
    } else if win_rate > 0.50 {
        5.0
    } else if win_rate > 0.40 {
        0.0
    } else if win_rate > 0.30 {
        -10.0
    } else {
        -20.0
    };

    let reason = if adjustment > 0.0 {
        format!(
            "📈 Historical: {:.0}% win rate over {} trades (avg ${:.2}) → +{:.0}",
            win_rate * 100.0, trade_count, avg_pnl, adjustment
        )
    } else if adjustment < 0.0 {
        format!(
            "📉 Historical: {:.0}% win rate over {} trades (avg ${:.2}) → {:.0}",
            win_rate * 100.0, trade_count, avg_pnl, adjustment
        )
    } else {
        format!(
            "📊 Historical: {:.0}% win rate over {} trades (neutral)",
            win_rate * 100.0, trade_count
        )
    };

    (adjustment, reason)
}
