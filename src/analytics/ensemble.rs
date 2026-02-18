//! Cross-Bot Ensemble Learning
//! 
//! Learns from collective bot intelligence across strategy clusters.
//! When one bot fails in specific conditions, all similar bots learn to avoid that pattern.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{info, warn};

/// Bot cluster identifier
#[derive(Debug, Clone)]
pub struct ClusterId {
    pub strategy_category: String, // trend, mean_reversion, breakout, liquidity_sweep
    pub instrument: String,        // XAU_USD, BTC_USD, etc.
}

impl ClusterId {
    pub fn new(strategy_category: &str, instrument: &str) -> Self {
        Self {
            strategy_category: strategy_category.to_string(),
            instrument: instrument.to_string(),
        }
    }
    
    pub fn as_string(&self) -> String {
        format!("{}_{}", self.strategy_category, self.instrument)
    }
}

/// Market pattern for clustering similar conditions
#[derive(Debug, Clone, Hash)]
pub struct MarketPattern {
    pub regime: String,
    pub session: Option<String>,
    pub rsi_zone: String, // oversold, neutral, overbought
    pub volatility_level: String, // low, normal, high
    pub trend_strength: String, // weak, moderate, strong
}

impl MarketPattern {
    /// Generate hash for pattern matching
    pub fn hash(&self) -> String {
        let mut hasher = DefaultHasher::new();
        Hash::hash(self, &mut hasher);
        format!("{:x}", hasher.finish())
    }
    
    /// Determine RSI zone
    pub fn rsi_zone(rsi: f64) -> String {
        if rsi < 35.0 {
            "oversold".to_string()
        } else if rsi > 65.0 {
            "overbought".to_string()
        } else {
            "neutral".to_string()
        }
    }
    
    /// Determine volatility level
    pub fn volatility_level(vol_ratio: f64) -> String {
        if vol_ratio < 0.7 {
            "low".to_string()
        } else if vol_ratio > 1.5 {
            "high".to_string()
        } else {
            "normal".to_string()
        }
    }
    
    /// Determine trend strength from ADX
    pub fn trend_strength(adx: f64) -> String {
        if adx < 20.0 {
            "weak".to_string()
        } else if adx < 30.0 {
            "moderate".to_string()
        } else {
            "strong".to_string()
        }
    }
}

/// Ensemble learning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleFeedback {
    pub confidence_multiplier: f64, // 0.8-1.2
    pub should_skip: bool,
    pub reason: Option<String>,
    pub cluster_win_rate: f64,
    pub recent_failures: usize,
}

impl Default for EnsembleFeedback {
    fn default() -> Self {
        Self {
            confidence_multiplier: 1.0,
            should_skip: false,
            reason: None,
            cluster_win_rate: 0.5, // Neutral
            recent_failures: 0,
        }
    }
}

/// Cross-bot ensemble engine
pub struct EnsembleEngine {
    pool: PgPool,
}

impl EnsembleEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Record a trade outcome for ensemble learning
    pub async fn record_trade_outcome(
        &self,
        cluster_id: &ClusterId,
        pattern: &MarketPattern,
        outcome: &str, // "win", "loss", "breakeven"
        confidence: f64,
        pnl: f64,
        trade_id: Option<uuid::Uuid>,
        bot_id: uuid::Uuid,
    ) -> Result<()> {
        let cluster_str = cluster_id.as_string();
        let pattern_hash = pattern.hash();
        
        sqlx::query!(
            r#"
            INSERT INTO cross_bot_learnings 
                (cluster_id, pattern_hash, regime, session, outcome, confidence, pnl, trade_id, bot_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            cluster_str,
            pattern_hash,
            pattern.regime,
            pattern.session,
            outcome,
            confidence as f32,
            pnl as f32,
            trade_id,
            bot_id
        )
        .execute(&self.pool)
        .await?;
        
        info!(
            "Ensemble: Recorded {} for cluster {} (pattern: {}, pnl: {:.2})",
            outcome, cluster_str, pattern_hash, pnl
        );
        
        Ok(())
    }
    
    /// Query ensemble feedback for a proposed trade
    pub async fn get_feedback(
        &self,
        cluster_id: &ClusterId,
        pattern: &MarketPattern,
        signal_confidence: f64,
    ) -> Result<EnsembleFeedback> {
        let cluster_str = cluster_id.as_string();
        let pattern_hash = pattern.hash();
        
        // Query recent outcomes for this cluster + pattern (last 30 days)
        let recent_trades = sqlx::query!(
            r#"
            SELECT outcome, confidence, pnl, created_at
            FROM cross_bot_learnings
            WHERE cluster_id = $1 
              AND pattern_hash = $2
              AND created_at > NOW() - INTERVAL '30 days'
            ORDER BY created_at DESC
            LIMIT 20
            "#,
            cluster_str,
            pattern_hash
        )
        .fetch_all(&self.pool)
        .await?;
        
        if recent_trades.is_empty() {
            // No history - return neutral feedback
            return Ok(EnsembleFeedback::default());
        }
        
        // Calculate cluster performance
        let wins = recent_trades.iter().filter(|t| t.outcome == "win").count();
        let losses = recent_trades.iter().filter(|t| t.outcome == "loss").count();
        let total = wins + losses;
        
        let win_rate = if total > 0 {
            wins as f64 / total as f64
        } else {
            0.5
        };
        
        // Count recent failures (last 3 trades)
        let recent_failures = recent_trades
            .iter()
            .take(3)
            .filter(|t| t.outcome == "loss")
            .count();
        
        // Calculate confidence multiplier
        let mut multiplier = 1.0;
        let mut should_skip = false;
        let mut reason = None;
        
        // Strong recent failure pattern = reduce confidence
        if recent_failures >= 3 {
            multiplier = 0.7;
            should_skip = signal_confidence * multiplier < 0.6;
            reason = Some(format!(
                "Cluster has 3 consecutive losses in this pattern (win rate: {:.0}%)",
                win_rate * 100.0
            ));
        } else if recent_failures == 2 && total >= 5 {
            multiplier = 0.85;
            reason = Some(format!(
                "Cluster has 2 recent losses in this pattern (win rate: {:.0}%)",
                win_rate * 100.0
            ));
        }
        // Strong positive history = boost confidence
        else if win_rate > 0.7 && total >= 10 {
            multiplier = 1.15;
            reason = Some(format!(
                "Cluster excels in this pattern (win rate: {:.0}%)",
                win_rate * 100.0
            ));
        }
        // Weak performance = reduce confidence
        else if win_rate < 0.4 && total >= 10 {
            multiplier = 0.85;
            reason = Some(format!(
                "Cluster struggles in this pattern (win rate: {:.0}%)",
                win_rate * 100.0
            ));
        }
        
        if multiplier != 1.0 {
            info!(
                "Ensemble: {} feedback for {} - multiplier: {:.2}x (win rate: {:.0}%, recent failures: {})",
                if multiplier > 1.0 { "Positive" } else { "Negative" },
                cluster_str,
                multiplier,
                win_rate * 100.0,
                recent_failures
            );
        }
        
        Ok(EnsembleFeedback {
            confidence_multiplier: multiplier,
            should_skip,
            reason,
            cluster_win_rate: win_rate,
            recent_failures,
        })
    }
    
    /// Get cluster performance summary
    pub async fn get_cluster_performance(&self, cluster_id: &ClusterId) -> Result<ClusterPerformance> {
        let cluster_str = cluster_id.as_string();
        
        let stats = sqlx::query!(
            r#"
            SELECT 
                regime,
                COUNT(*) as total_trades,
                SUM(CASE WHEN outcome = 'win' THEN 1 ELSE 0 END) as wins,
                AVG(CASE WHEN outcome = 'win' THEN 1.0 ELSE 0.0 END) as win_rate,
                AVG(pnl) as avg_pnl,
                SUM(pnl) as total_pnl
            FROM cross_bot_learnings
            WHERE cluster_id = $1
              AND created_at > NOW() - INTERVAL '30 days'
            GROUP BY regime
            ORDER BY total_trades DESC
            "#,
            cluster_str
        )
        .fetch_all(&self.pool)
        .await?;
        
        let by_regime = stats
            .into_iter()
            .map(|row| RegimeStats {
                regime: row.regime,
                total_trades: row.total_trades.unwrap_or(0) as usize,
                wins: row.wins.unwrap_or(0) as usize,
                win_rate: row.win_rate.map(|v| v.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
                avg_pnl: row.avg_pnl.unwrap_or(0.0),
                total_pnl: row.total_pnl.unwrap_or(0.0) as f64,
            })
            .collect();
        
        Ok(ClusterPerformance {
            cluster_id: cluster_str,
            by_regime,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterPerformance {
    pub cluster_id: String,
    pub by_regime: Vec<RegimeStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegimeStats {
    pub regime: String,
    pub total_trades: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub avg_pnl: f64,
    pub total_pnl: f64,
}
