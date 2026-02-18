//! Performance Analytics - Win rate analysis by condition
//!
//! Provides insights into what market conditions lead to winning trades

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Overall performance summary for a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotPerformanceSummary {
    pub bot_id: Uuid,
    pub total_trades: i64,
    pub winners: i64,
    pub losers: i64,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_pnl: f64,
    pub avg_winner_pnl: f64,
    pub avg_loser_pnl: f64,
    pub profit_factor: f64,
    pub avg_trade_duration_minutes: f64,
}

/// Performance breakdown by a specific condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionPerformance {
    pub condition: String,
    pub bucket: String,
    pub total_trades: i64,
    pub winners: i64,
    pub losers: i64,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_pnl: f64,
}

/// Full analytics report for a bot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotAnalyticsReport {
    pub summary: BotPerformanceSummary,
    pub by_rsi: Vec<ConditionPerformance>,
    pub by_adx: Vec<ConditionPerformance>,
    pub by_regime: Vec<ConditionPerformance>,
    pub by_session: Vec<ConditionPerformance>,
    pub by_hour: Vec<ConditionPerformance>,
    pub by_day: Vec<ConditionPerformance>,
    pub by_volatility: Vec<ConditionPerformance>,
    pub recommendations: Vec<Recommendation>,
}

/// A recommendation for parameter adjustment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub parameter: String,
    pub current_value: String,
    pub suggested_value: String,
    pub reason: String,
    pub expected_improvement: String,
    pub confidence: f64,
    pub trades_analyzed: i64,
}

/// Get overall performance summary for a bot
pub async fn get_bot_performance_summary(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<BotPerformanceSummary, sqlx::Error> {
    let row = sqlx::query_as::<_, (i64, i64, i64, Option<f64>, Option<f64>, Option<f64>, Option<f64>)>(
        r#"
        SELECT 
            COUNT(*) as total_trades,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl,
            (AVG(pnl) FILTER (WHERE is_winner = true))::FLOAT8 as avg_winner,
            (AVG(pnl) FILTER (WHERE is_winner = false))::FLOAT8 as avg_loser,
            AVG(trade_duration_minutes)::FLOAT8 as avg_duration
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL
        "#
    )
    .bind(bot_id)
    .fetch_one(pool)
    .await?;
    
    let (total, winners, losers, total_pnl, avg_winner, avg_loser, avg_duration) = row;
    
    let win_rate = if total > 0 { winners as f64 / total as f64 } else { 0.0 };
    let avg_pnl = if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 };
    
    // Profit factor = gross profit / gross loss
    let gross_profit = avg_winner.unwrap_or(0.0) * winners as f64;
    let gross_loss = avg_loser.unwrap_or(0.0).abs() * losers as f64;
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        999.99  // Perfect record (no losses) - cap display value
    } else {
        0.0
    };
    
    Ok(BotPerformanceSummary {
        bot_id,
        total_trades: total,
        winners,
        losers,
        win_rate,
        total_pnl: total_pnl.unwrap_or(0.0),
        avg_pnl,
        avg_winner_pnl: avg_winner.unwrap_or(0.0),
        avg_loser_pnl: avg_loser.unwrap_or(0.0),
        profit_factor,
        avg_trade_duration_minutes: avg_duration.unwrap_or(0.0),
    })
}

/// Get performance breakdown by RSI buckets
pub async fn get_performance_by_rsi(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            CASE 
                WHEN rsi_at_entry < 20 THEN '0-20 (Extreme Oversold)'
                WHEN rsi_at_entry < 30 THEN '20-30 (Oversold)'
                WHEN rsi_at_entry < 40 THEN '30-40 (Weak)'
                WHEN rsi_at_entry < 50 THEN '40-50 (Neutral Low)'
                WHEN rsi_at_entry < 60 THEN '50-60 (Neutral High)'
                WHEN rsi_at_entry < 70 THEN '60-70 (Strong)'
                WHEN rsi_at_entry < 80 THEN '70-80 (Overbought)'
                ELSE '80-100 (Extreme Overbought)'
            END as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL AND rsi_at_entry IS NOT NULL
        GROUP BY bucket
        ORDER BY bucket
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "RSI".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by ADX buckets
pub async fn get_performance_by_adx(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            CASE 
                WHEN adx_at_entry < 15 THEN '0-15 (No Trend)'
                WHEN adx_at_entry < 20 THEN '15-20 (Weak Trend)'
                WHEN adx_at_entry < 25 THEN '20-25 (Developing)'
                WHEN adx_at_entry < 30 THEN '25-30 (Trending)'
                WHEN adx_at_entry < 40 THEN '30-40 (Strong Trend)'
                ELSE '40+ (Very Strong)'
            END as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL AND adx_at_entry IS NOT NULL
        GROUP BY bucket
        ORDER BY bucket
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "ADX".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by MRATE regime
pub async fn get_performance_by_regime(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            COALESCE(mrate_regime, 'NO_MRATE') as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL
        GROUP BY bucket
        ORDER BY total DESC
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "MRATE Regime".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by trading session
pub async fn get_performance_by_session(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            COALESCE(trading_session, 'UNKNOWN') as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL
        GROUP BY bucket
        ORDER BY total DESC
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "Trading Session".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by hour of day
pub async fn get_performance_by_hour(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i32, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            entry_hour,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL
        GROUP BY entry_hour
        ORDER BY entry_hour
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(hour, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "Hour (UTC)".to_string(),
            bucket: format!("{:02}:00", hour),
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by day of week
pub async fn get_performance_by_day(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i32, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            entry_day_of_week,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL
        GROUP BY entry_day_of_week
        ORDER BY entry_day_of_week
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    let day_names = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    
    Ok(rows.into_iter().map(|(day, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "Day of Week".to_string(),
            bucket: day_names.get(day as usize).unwrap_or(&"Unknown").to_string(),
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get performance breakdown by volatility regime
pub async fn get_performance_by_volatility(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            CASE 
                WHEN volatility_ratio < 0.5 THEN 'Very Low (<0.5x)'
                WHEN volatility_ratio < 0.8 THEN 'Low (0.5-0.8x)'
                WHEN volatility_ratio < 1.2 THEN 'Normal (0.8-1.2x)'
                WHEN volatility_ratio < 1.5 THEN 'High (1.2-1.5x)'
                WHEN volatility_ratio < 2.0 THEN 'Very High (1.5-2x)'
                ELSE 'Extreme (>2x)'
            END as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE bot_id = $1 AND is_winner IS NOT NULL AND volatility_ratio IS NOT NULL
        GROUP BY bucket
        ORDER BY bucket
        "#
    )
    .bind(bot_id)
    .fetch_all(pool)
    .await?;
    
    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "Volatility".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Generate recommendations based on performance data
pub fn generate_recommendations(
    summary: &BotPerformanceSummary,
    by_rsi: &[ConditionPerformance],
    by_adx: &[ConditionPerformance],
    by_regime: &[ConditionPerformance],
    by_session: &[ConditionPerformance],
    by_hour: &[ConditionPerformance],
) -> Vec<Recommendation> {
    let mut recommendations = Vec::new();
    let min_trades = 5; // Minimum trades to make a recommendation
    
    // Check RSI performance
    for perf in by_rsi {
        if perf.total_trades >= min_trades {
            // If a bucket has significantly better win rate
            if perf.win_rate > summary.win_rate + 0.15 {
                recommendations.push(Recommendation {
                    parameter: "RSI Entry Zone".to_string(),
                    current_value: "Current setting".to_string(),
                    suggested_value: format!("Focus on {} range", perf.bucket),
                    reason: format!(
                        "Trades in {} RSI range have {:.0}% win rate vs {:.0}% overall",
                        perf.bucket, perf.win_rate * 100.0, summary.win_rate * 100.0
                    ),
                    expected_improvement: format!("+{:.0}% win rate", (perf.win_rate - summary.win_rate) * 100.0),
                    confidence: (perf.total_trades as f64 / 20.0).min(1.0),
                    trades_analyzed: perf.total_trades,
                });
            }
            // If a bucket has significantly worse win rate
            if perf.win_rate < summary.win_rate - 0.15 && perf.total_trades >= min_trades {
                recommendations.push(Recommendation {
                    parameter: "RSI Avoidance".to_string(),
                    current_value: "Trading all RSI levels".to_string(),
                    suggested_value: format!("Avoid {} range", perf.bucket),
                    reason: format!(
                        "Trades in {} RSI range have only {:.0}% win rate",
                        perf.bucket, perf.win_rate * 100.0
                    ),
                    expected_improvement: format!("Avoid {:.0}% of losing trades", (1.0 - perf.win_rate) * 100.0),
                    confidence: (perf.total_trades as f64 / 20.0).min(1.0),
                    trades_analyzed: perf.total_trades,
                });
            }
        }
    }
    
    // Check regime performance
    for perf in by_regime {
        if perf.total_trades >= min_trades && perf.win_rate < 0.40 {
            recommendations.push(Recommendation {
                parameter: "MRATE Regime Filter".to_string(),
                current_value: "Trading in all regimes".to_string(),
                suggested_value: format!("Avoid {} regime", perf.bucket),
                reason: format!(
                    "Trades in {} regime have only {:.0}% win rate with ${:.2} avg loss",
                    perf.bucket, perf.win_rate * 100.0, perf.avg_pnl
                ),
                expected_improvement: "Reduce losses in unfavorable regimes".to_string(),
                confidence: (perf.total_trades as f64 / 20.0).min(1.0),
                trades_analyzed: perf.total_trades,
            });
        }
    }
    
    // Check session performance
    for perf in by_session {
        if perf.total_trades >= min_trades && perf.win_rate < 0.40 {
            recommendations.push(Recommendation {
                parameter: "Trading Session".to_string(),
                current_value: "Trading all sessions".to_string(),
                suggested_value: format!("Avoid {} session", perf.bucket),
                reason: format!(
                    "{} session has only {:.0}% win rate",
                    perf.bucket, perf.win_rate * 100.0
                ),
                expected_improvement: "Focus on better-performing sessions".to_string(),
                confidence: (perf.total_trades as f64 / 20.0).min(1.0),
                trades_analyzed: perf.total_trades,
            });
        }
    }
    
    // Check hourly performance - find worst hours
    let bad_hours: Vec<_> = by_hour.iter()
        .filter(|h| h.total_trades >= min_trades && h.win_rate < 0.35)
        .collect();
    
    if !bad_hours.is_empty() {
        let hours_str = bad_hours.iter()
            .map(|h| h.bucket.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let total_trades: i64 = bad_hours.iter().map(|h| h.total_trades).sum();
        
        recommendations.push(Recommendation {
            parameter: "Trading Hours".to_string(),
            current_value: "Trading all hours".to_string(),
            suggested_value: format!("Avoid hours: {}", hours_str),
            reason: "These hours consistently underperform".to_string(),
            expected_improvement: "Reduce trades during unfavorable times".to_string(),
            confidence: (total_trades as f64 / 30.0).min(1.0),
            trades_analyzed: total_trades,
        });
    }
    
    // Check ADX performance
    for perf in by_adx {
        if perf.total_trades >= min_trades {
            if perf.bucket.contains("No Trend") && perf.win_rate < 0.45 {
                recommendations.push(Recommendation {
                    parameter: "ADX Filter".to_string(),
                    current_value: "Trading in low ADX".to_string(),
                    suggested_value: "Require ADX > 15 or 20".to_string(),
                    reason: format!(
                        "Trades with ADX < 15 have only {:.0}% win rate",
                        perf.win_rate * 100.0
                    ),
                    expected_improvement: "Filter out choppy market trades".to_string(),
                    confidence: (perf.total_trades as f64 / 20.0).min(1.0),
                    trades_analyzed: perf.total_trades,
                });
            }
        }
    }
    
    // Sort by confidence
    recommendations.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    
    recommendations
}

/// Get full analytics report for a bot
pub async fn get_bot_analytics_report(
    pool: &PgPool,
    bot_id: Uuid,
) -> Result<BotAnalyticsReport, sqlx::Error> {
    let summary = get_bot_performance_summary(pool, bot_id).await?;
    let by_rsi = get_performance_by_rsi(pool, bot_id).await?;
    let by_adx = get_performance_by_adx(pool, bot_id).await?;
    let by_regime = get_performance_by_regime(pool, bot_id).await?;
    let by_session = get_performance_by_session(pool, bot_id).await?;
    let by_hour = get_performance_by_hour(pool, bot_id).await?;
    let by_day = get_performance_by_day(pool, bot_id).await?;
    let by_volatility = get_performance_by_volatility(pool, bot_id).await?;
    
    let recommendations = generate_recommendations(
        &summary, &by_rsi, &by_adx, &by_regime, &by_session, &by_hour
    );
    
    Ok(BotAnalyticsReport {
        summary,
        by_rsi,
        by_adx,
        by_regime,
        by_session,
        by_hour,
        by_day,
        by_volatility,
        recommendations,
    })
}

/// Regime accuracy validation: win rate by regime × favored instrument
/// Shows whether MRATE regime classification is actually helping trade selection
pub async fn get_regime_accuracy(
    pool: &PgPool,
) -> Result<Vec<ConditionPerformance>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
        r#"
        SELECT 
            COALESCE(mrate_regime, 'NO_MRATE') || ' / ' || COALESCE(mrate_favored_instrument, 'NONE') as bucket,
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE is_winner = true) as winners,
            COUNT(*) FILTER (WHERE is_winner = false) as losers,
            SUM(pnl)::FLOAT8 as total_pnl
        FROM trade_context
        WHERE is_winner IS NOT NULL
        GROUP BY bucket
        HAVING COUNT(*) >= 3
        ORDER BY total DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(bucket, total, winners, losers, total_pnl)| {
        ConditionPerformance {
            condition: "Regime × Instrument".to_string(),
            bucket,
            total_trades: total,
            winners,
            losers,
            win_rate: if total > 0 { winners as f64 / total as f64 } else { 0.0 },
            total_pnl: total_pnl.unwrap_or(0.0),
            avg_pnl: if total > 0 { total_pnl.unwrap_or(0.0) / total as f64 } else { 0.0 },
        }
    }).collect())
}

/// Get analytics for all bots (summary only)
pub async fn get_all_bots_summary(
    pool: &PgPool,
) -> Result<Vec<(Uuid, String, BotPerformanceSummary)>, sqlx::Error> {
    // Get all bots with trades
    let bot_ids: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT DISTINCT tc.bot_id, b.name
        FROM trade_context tc
        JOIN bots b ON b.id = tc.bot_id
        WHERE tc.is_winner IS NOT NULL
        "#
    )
    .fetch_all(pool)
    .await?;
    
    let mut results = Vec::new();
    for (bot_id, name) in bot_ids {
        if let Ok(summary) = get_bot_performance_summary(pool, bot_id).await {
            results.push((bot_id, name, summary));
        }
    }
    
    Ok(results)
}
