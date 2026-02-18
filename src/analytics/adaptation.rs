//! Adaptation Engine - Auto-adjusts strategy parameters based on trade performance
//!
//! Analyzes trade context data and creates adaptation records that can be
//! applied to strategies to improve their performance.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::performance::{
    get_performance_by_adx, get_performance_by_hour, get_performance_by_regime,
    get_performance_by_rsi, get_performance_by_session, get_bot_performance_summary,
    ConditionPerformance,
};

/// Configuration for the adaptation engine
#[derive(Debug, Clone)]
pub struct AdaptationConfig {
    /// Minimum trades required before making an adaptation
    pub min_trades_for_adaptation: i64,
    /// How many days to look back for analysis
    pub lookback_days: i64,
    /// Minimum win rate difference to trigger adaptation (e.g., 0.15 = 15%)
    pub min_win_rate_diff: f64,
    /// Minimum confidence level to create adaptation
    pub min_confidence: f64,
    /// Whether to auto-apply high-confidence adaptations
    pub auto_apply_enabled: bool,
    /// Confidence threshold for auto-apply
    pub auto_apply_confidence: f64,
}

impl Default for AdaptationConfig {
    fn default() -> Self {
        Self {
            min_trades_for_adaptation: 10,
            lookback_days: 14,
            min_win_rate_diff: 0.12,
            min_confidence: 0.5,
            auto_apply_enabled: false,
            auto_apply_confidence: 0.8,
        }
    }
}

/// A strategy adaptation record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategyAdaptation {
    pub id: Uuid,
    pub strategy_id: Option<Uuid>,
    pub bot_id: Option<Uuid>,
    pub parameter_name: String,
    pub original_value: f64,
    pub adapted_value: f64,
    pub reason: String,
    pub condition_analyzed: Option<String>,
    pub trades_analyzed: i32,
    pub original_win_rate: Option<f64>,
    pub adapted_win_rate: Option<f64>,
    pub confidence: Option<f64>,
    pub status: String,
    pub approved_by: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub applied_at: Option<chrono::DateTime<Utc>>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
}

/// Proposed adaptation before it's saved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAdaptation {
    pub parameter_name: String,
    pub original_value: f64,
    pub adapted_value: f64,
    pub reason: String,
    pub condition_analyzed: String,
    pub trades_analyzed: i64,
    pub original_win_rate: f64,
    pub adapted_win_rate: f64,
    pub confidence: f64,
}

/// The adaptation engine
pub struct AdaptationEngine {
    config: AdaptationConfig,
}

impl AdaptationEngine {
    pub fn new(config: AdaptationConfig) -> Self {
        Self { config }
    }

    /// Analyze a bot's performance and generate proposed adaptations
    pub async fn analyze_and_propose(
        &self,
        pool: &PgPool,
        bot_id: Uuid,
        strategy_id: Uuid,
    ) -> Result<Vec<ProposedAdaptation>, sqlx::Error> {
        let mut proposals = Vec::new();

        // Get overall summary
        let summary = get_bot_performance_summary(pool, bot_id).await?;
        
        if summary.total_trades < self.config.min_trades_for_adaptation {
            return Ok(proposals);
        }

        // Analyze RSI performance
        let by_rsi = get_performance_by_rsi(pool, bot_id).await?;
        proposals.extend(self.analyze_rsi_performance(&by_rsi, summary.win_rate));

        // Analyze ADX performance
        let by_adx = get_performance_by_adx(pool, bot_id).await?;
        proposals.extend(self.analyze_adx_performance(&by_adx, summary.win_rate));

        // Analyze regime performance
        let by_regime = get_performance_by_regime(pool, bot_id).await?;
        proposals.extend(self.analyze_regime_performance(&by_regime, summary.win_rate));

        // Analyze session performance
        let by_session = get_performance_by_session(pool, bot_id).await?;
        proposals.extend(self.analyze_session_performance(&by_session, summary.win_rate));

        // Analyze hourly performance
        let by_hour = get_performance_by_hour(pool, bot_id).await?;
        proposals.extend(self.analyze_hourly_performance(&by_hour, summary.win_rate));

        Ok(proposals)
    }

    fn analyze_rsi_performance(
        &self,
        data: &[ConditionPerformance],
        overall_win_rate: f64,
    ) -> Vec<ProposedAdaptation> {
        let mut proposals = Vec::new();
        let min_trades = self.config.min_trades_for_adaptation;

        for perf in data {
            if perf.total_trades < min_trades {
                continue;
            }

            let diff = perf.win_rate - overall_win_rate;

            // If this RSI zone significantly underperforms, propose avoidance
            if diff < -self.config.min_win_rate_diff {
                // Extract RSI range from bucket name
                if let Some((min_rsi, max_rsi)) = extract_range(&perf.bucket) {
                    proposals.push(ProposedAdaptation {
                        parameter_name: "rsi_avoidance_zone".to_string(),
                        original_value: 0.0,
                        adapted_value: min_rsi,
                        reason: format!(
                            "Trades with RSI in {} have {:.0}% win rate vs {:.0}% overall. Avoid this zone.",
                            perf.bucket,
                            perf.win_rate * 100.0,
                            overall_win_rate * 100.0
                        ),
                        condition_analyzed: format!("rsi_at_entry BETWEEN {} AND {}", min_rsi, max_rsi),
                        trades_analyzed: perf.total_trades,
                        original_win_rate: overall_win_rate,
                        adapted_win_rate: perf.win_rate,
                        confidence: calculate_confidence(perf.total_trades, diff.abs()),
                    });
                }
            }

            // If this RSI zone significantly outperforms, propose focusing on it
            if diff > self.config.min_win_rate_diff {
                if let Some((min_rsi, _max_rsi)) = extract_range(&perf.bucket) {
                    proposals.push(ProposedAdaptation {
                        parameter_name: "rsi_focus_zone".to_string(),
                        original_value: 0.0,
                        adapted_value: min_rsi,
                        reason: format!(
                            "Trades with RSI in {} have {:.0}% win rate vs {:.0}% overall. Focus here.",
                            perf.bucket,
                            perf.win_rate * 100.0,
                            overall_win_rate * 100.0
                        ),
                        condition_analyzed: format!("rsi_at_entry in {}", perf.bucket),
                        trades_analyzed: perf.total_trades,
                        original_win_rate: overall_win_rate,
                        adapted_win_rate: perf.win_rate,
                        confidence: calculate_confidence(perf.total_trades, diff),
                    });
                }
            }
        }

        proposals
    }

    fn analyze_adx_performance(
        &self,
        data: &[ConditionPerformance],
        overall_win_rate: f64,
    ) -> Vec<ProposedAdaptation> {
        let mut proposals = Vec::new();
        let min_trades = self.config.min_trades_for_adaptation;

        for perf in data {
            if perf.total_trades < min_trades {
                continue;
            }

            let diff = perf.win_rate - overall_win_rate;

            // Low ADX (no trend) with bad performance -> require higher ADX
            if perf.bucket.contains("No Trend") && diff < -self.config.min_win_rate_diff {
                proposals.push(ProposedAdaptation {
                    parameter_name: "adx_minimum".to_string(),
                    original_value: 0.0,
                    adapted_value: 20.0,
                    reason: format!(
                        "Trades with ADX < 15 have only {:.0}% win rate. Require ADX > 20.",
                        perf.win_rate * 100.0
                    ),
                    condition_analyzed: "adx_at_entry < 15".to_string(),
                    trades_analyzed: perf.total_trades,
                    original_win_rate: overall_win_rate,
                    adapted_win_rate: perf.win_rate,
                    confidence: calculate_confidence(perf.total_trades, diff.abs()),
                });
            }

            // Strong trend with good performance -> prefer trending markets
            if perf.bucket.contains("Strong") && diff > self.config.min_win_rate_diff {
                proposals.push(ProposedAdaptation {
                    parameter_name: "adx_preferred_min".to_string(),
                    original_value: 0.0,
                    adapted_value: 25.0,
                    reason: format!(
                        "Trades with strong ADX have {:.0}% win rate vs {:.0}% overall.",
                        perf.win_rate * 100.0,
                        overall_win_rate * 100.0
                    ),
                    condition_analyzed: "adx_at_entry > 30".to_string(),
                    trades_analyzed: perf.total_trades,
                    original_win_rate: overall_win_rate,
                    adapted_win_rate: perf.win_rate,
                    confidence: calculate_confidence(perf.total_trades, diff),
                });
            }
        }

        proposals
    }

    fn analyze_regime_performance(
        &self,
        data: &[ConditionPerformance],
        overall_win_rate: f64,
    ) -> Vec<ProposedAdaptation> {
        let mut proposals = Vec::new();
        let min_trades = self.config.min_trades_for_adaptation;

        for perf in data {
            if perf.total_trades < min_trades {
                continue;
            }

            let diff = perf.win_rate - overall_win_rate;

            // Regime with significantly bad performance -> avoid
            if diff < -self.config.min_win_rate_diff && perf.win_rate < 0.40 {
                proposals.push(ProposedAdaptation {
                    parameter_name: "regime_avoidance".to_string(),
                    original_value: 0.0,
                    adapted_value: 1.0, // Flag to enable avoidance
                    reason: format!(
                        "Trades in {} regime have only {:.0}% win rate with ${:.2} avg loss. Avoid this regime.",
                        perf.bucket,
                        perf.win_rate * 100.0,
                        perf.avg_pnl
                    ),
                    condition_analyzed: format!("mrate_regime = '{}'", perf.bucket),
                    trades_analyzed: perf.total_trades,
                    original_win_rate: overall_win_rate,
                    adapted_win_rate: perf.win_rate,
                    confidence: calculate_confidence(perf.total_trades, diff.abs()),
                });
            }
        }

        proposals
    }

    fn analyze_session_performance(
        &self,
        data: &[ConditionPerformance],
        overall_win_rate: f64,
    ) -> Vec<ProposedAdaptation> {
        let mut proposals = Vec::new();
        let min_trades = self.config.min_trades_for_adaptation;

        for perf in data {
            if perf.total_trades < min_trades {
                continue;
            }

            let diff = perf.win_rate - overall_win_rate;

            // Session with bad performance -> avoid
            if diff < -self.config.min_win_rate_diff && perf.win_rate < 0.40 {
                proposals.push(ProposedAdaptation {
                    parameter_name: "session_avoidance".to_string(),
                    original_value: 0.0,
                    adapted_value: 1.0,
                    reason: format!(
                        "{} session has only {:.0}% win rate. Consider avoiding.",
                        perf.bucket,
                        perf.win_rate * 100.0
                    ),
                    condition_analyzed: format!("trading_session = '{}'", perf.bucket),
                    trades_analyzed: perf.total_trades,
                    original_win_rate: overall_win_rate,
                    adapted_win_rate: perf.win_rate,
                    confidence: calculate_confidence(perf.total_trades, diff.abs()),
                });
            }
        }

        proposals
    }

    fn analyze_hourly_performance(
        &self,
        data: &[ConditionPerformance],
        overall_win_rate: f64,
    ) -> Vec<ProposedAdaptation> {
        let mut proposals = Vec::new();
        let min_trades = self.config.min_trades_for_adaptation / 2; // Lower threshold for hours

        // Collect bad hours
        let bad_hours: Vec<&ConditionPerformance> = data
            .iter()
            .filter(|p| p.total_trades >= min_trades && p.win_rate < 0.35)
            .collect();

        if bad_hours.len() >= 2 {
            let hours_str = bad_hours
                .iter()
                .map(|h| h.bucket.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let total_trades: i64 = bad_hours.iter().map(|h| h.total_trades).sum();
            let avg_win_rate = bad_hours.iter().map(|h| h.win_rate).sum::<f64>() / bad_hours.len() as f64;

            proposals.push(ProposedAdaptation {
                parameter_name: "hour_avoidance".to_string(),
                original_value: 0.0,
                adapted_value: 1.0,
                reason: format!(
                    "Hours {} consistently underperform with {:.0}% avg win rate.",
                    hours_str,
                    avg_win_rate * 100.0
                ),
                condition_analyzed: format!("entry_hour IN ({})", 
                    bad_hours.iter()
                        .filter_map(|h| h.bucket.split(':').next().and_then(|s| s.parse::<i32>().ok()))
                        .map(|h| h.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                trades_analyzed: total_trades,
                original_win_rate: overall_win_rate,
                adapted_win_rate: avg_win_rate,
                confidence: calculate_confidence(total_trades, (overall_win_rate - avg_win_rate).abs()),
            });
        }

        proposals
    }
}

/// Save a proposed adaptation to the database
pub async fn save_adaptation(
    pool: &PgPool,
    bot_id: Uuid,
    strategy_id: Uuid,
    proposal: &ProposedAdaptation,
    auto_apply: bool,
) -> Result<StrategyAdaptation, sqlx::Error> {
    let status = if auto_apply { "AUTO_APPLIED" } else { "PENDING" };
    let approved_by = if auto_apply { Some("AUTO") } else { None };
    let applied_at = if auto_apply { Some(Utc::now()) } else { None };
    let expires_at = Utc::now() + Duration::days(14);

    let adaptation = sqlx::query_as::<_, StrategyAdaptation>(
        r#"
        INSERT INTO strategy_adaptations (
            bot_id, strategy_id, parameter_name, original_value, adapted_value,
            reason, condition_analyzed, trades_analyzed, original_win_rate,
            adapted_win_rate, confidence, status, approved_by, applied_at, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING *
        "#,
    )
    .bind(bot_id)
    .bind(strategy_id)
    .bind(&proposal.parameter_name)
    .bind(proposal.original_value)
    .bind(proposal.adapted_value)
    .bind(&proposal.reason)
    .bind(&proposal.condition_analyzed)
    .bind(proposal.trades_analyzed as i32)
    .bind(proposal.original_win_rate)
    .bind(proposal.adapted_win_rate)
    .bind(proposal.confidence)
    .bind(status)
    .bind(approved_by)
    .bind(applied_at)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok(adaptation)
}

/// Get active (approved/auto-applied) adaptations for a strategy
pub async fn get_active_adaptations(
    pool: &PgPool,
    strategy_id: Uuid,
) -> Result<Vec<StrategyAdaptation>, sqlx::Error> {
    let adaptations = sqlx::query_as::<_, StrategyAdaptation>(
        r#"
        SELECT * FROM strategy_adaptations
        WHERE strategy_id = $1
          AND status IN ('APPROVED', 'AUTO_APPLIED')
          AND (expires_at IS NULL OR expires_at > NOW())
        ORDER BY created_at DESC
        "#,
    )
    .bind(strategy_id)
    .fetch_all(pool)
    .await?;

    Ok(adaptations)
}

/// Get pending adaptations for a bot
pub async fn get_pending_adaptations(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<StrategyAdaptation>, sqlx::Error> {
    let adaptations = sqlx::query_as::<_, StrategyAdaptation>(
        r#"
        SELECT * FROM strategy_adaptations
        WHERE bot_id = $1 AND status = 'PENDING'
        ORDER BY confidence DESC, created_at DESC
        "#,
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;

    Ok(adaptations)
}

/// Approve an adaptation
pub async fn approve_adaptation(
    pool: &PgPool,
    adaptation_id: Uuid,
    approved_by: &str,
) -> Result<StrategyAdaptation, sqlx::Error> {
    let adaptation = sqlx::query_as::<_, StrategyAdaptation>(
        r#"
        UPDATE strategy_adaptations
        SET status = 'APPROVED', approved_by = $2, applied_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(adaptation_id)
    .bind(approved_by)
    .fetch_one(pool)
    .await?;

    Ok(adaptation)
}

/// Reject an adaptation
pub async fn reject_adaptation(
    pool: &PgPool,
    adaptation_id: Uuid,
) -> Result<StrategyAdaptation, sqlx::Error> {
    let adaptation = sqlx::query_as::<_, StrategyAdaptation>(
        r#"
        UPDATE strategy_adaptations
        SET status = 'REJECTED'
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(adaptation_id)
    .fetch_one(pool)
    .await?;

    Ok(adaptation)
}

/// Helper to extract numeric range from bucket string like "20-30 (Oversold)"
fn extract_range(bucket: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = bucket.split('-').collect();
    if parts.len() >= 2 {
        let min = parts[0].trim().parse::<f64>().ok()?;
        let max = parts[1]
            .split(|c: char| !c.is_numeric() && c != '.')
            .next()?
            .trim()
            .parse::<f64>()
            .ok()?;
        Some((min, max))
    } else {
        None
    }
}

/// Calculate confidence based on trade count and performance difference
fn calculate_confidence(trades: i64, diff: f64) -> f64 {
    // More trades = more confidence
    let trade_factor = (trades as f64 / 50.0).min(1.0);
    // Bigger difference = more confidence
    let diff_factor = (diff / 0.30).min(1.0);
    
    (trade_factor * 0.6 + diff_factor * 0.4).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_range() {
        assert_eq!(extract_range("20-30 (Oversold)"), Some((20.0, 30.0)));
        assert_eq!(extract_range("0-20 (Extreme)"), Some((0.0, 20.0)));
        assert_eq!(extract_range("invalid"), None);
    }

    #[test]
    fn test_calculate_confidence() {
        // 50 trades, 30% diff -> high confidence
        let conf = calculate_confidence(50, 0.30);
        assert!(conf > 0.9);
        
        // 10 trades, 10% diff -> low confidence
        let conf = calculate_confidence(10, 0.10);
        assert!(conf < 0.5);
    }
}
