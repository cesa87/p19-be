//! Strategy correlation analysis
//!
//! Prevents multiple correlated strategies from trading simultaneously
//! This is a Phase 4 feature - stub implementation for now

use sqlx::PgPool;
use uuid::Uuid;

/// Correlation engine for strategy diversification
pub struct CorrelationEngine {
    lookback_trades: usize,      // e.g., 30
    correlation_threshold: f64,  // e.g., 0.7
}

impl Default for CorrelationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrelationEngine {
    pub fn new() -> Self {
        Self {
            lookback_trades: 30,
            correlation_threshold: 0.7,
        }
    }
    
    pub fn with_params(lookback: usize, threshold: f64) -> Self {
        Self {
            lookback_trades: lookback,
            correlation_threshold: threshold,
        }
    }
    
    /// Calculate correlation between two strategies
    /// 
    /// Returns correlation coefficient (-1.0 to 1.0)
    /// Uses recent trade P&L to calculate correlation
    pub async fn calculate_strategy_correlation(
        &self,
        pool: &PgPool,
        strategy_a: Uuid,
        strategy_b: Uuid,
    ) -> Result<f64, String> {
        // Fetch recent trades for both strategies
        let trades_a = self.fetch_strategy_returns(pool, strategy_a).await?;
        let trades_b = self.fetch_strategy_returns(pool, strategy_b).await?;
        
        if trades_a.len() < 5 || trades_b.len() < 5 {
            // Not enough data - assume uncorrelated
            return Ok(0.0);
        }
        
        // Calculate correlation
        let corr = pearson_correlation(&trades_a, &trades_b);
        Ok(corr)
    }
    
    /// Fetch recent trade returns for a strategy
    async fn fetch_strategy_returns(
        &self,
        pool: &PgPool,
        strategy_id: Uuid,
    ) -> Result<Vec<f64>, String> {
        let returns: Vec<(f64,)> = sqlx::query_as(
            r#"
            SELECT pnl::FLOAT8
            FROM trade_context
            WHERE strategy_id = $1
              AND pnl IS NOT NULL
              AND closed_at IS NOT NULL
            ORDER BY closed_at DESC
            LIMIT $2
            "#
        )
        .bind(strategy_id)
        .bind(self.lookback_trades as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to fetch strategy returns: {}", e))?;
        
        Ok(returns.into_iter().map(|(pnl,)| pnl).collect())
    }
    
    /// Check if a new strategy is correlated with any open positions
    /// 
    /// Returns true if correlation > threshold with any open strategy
    pub async fn is_correlated(
        &self,
        pool: &PgPool,
        new_strategy: Uuid,
        open_strategy_ids: &[Uuid],
    ) -> Result<bool, String> {
        if open_strategy_ids.is_empty() {
            return Ok(false);
        }
        
        // Check correlation against each open strategy
        for &open_strategy in open_strategy_ids {
            if open_strategy == new_strategy {
                // Same strategy - skip (already checked by position limits)
                continue;
            }
            
            let corr = self.calculate_strategy_correlation(
                pool,
                new_strategy,
                open_strategy,
            ).await?;
            
            if corr.abs() > self.correlation_threshold {
                tracing::warn!(
                    "Strategy correlation detected: {} <-> {} = {:.3}",
                    new_strategy, open_strategy, corr
                );
                return Ok(true);
            }
        }
        
        Ok(false)
    }
}

/// Calculate Pearson correlation coefficient
#[allow(dead_code)]
fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len().min(y.len()) as f64;
    if n == 0.0 {
        return 0.0;
    }
    
    let mean_x: f64 = x.iter().sum::<f64>() / n;
    let mean_y: f64 = y.iter().sum::<f64>() / n;
    
    let mut numerator = 0.0;
    let mut denom_x = 0.0;
    let mut denom_y = 0.0;
    
    for i in 0..(n as usize) {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        numerator += dx * dy;
        denom_x += dx * dx;
        denom_y += dy * dy;
    }
    
    if denom_x == 0.0 || denom_y == 0.0 {
        return 0.0;
    }
    
    numerator / (denom_x.sqrt() * denom_y.sqrt())
}
