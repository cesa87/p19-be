//! Dynamic Parameter Evolution
//! 
//! Genetic algorithm that evolves bot parameters over time.
//! Top performers "breed" variants with mutated parameters.
//! Successful variants replace underperforming parents.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

/// Bot variant for A/B testing evolved parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotVariant {
    pub id: Uuid,
    pub parent_bot_id: Uuid,
    pub variant_name: String,
    pub generation: i32,
    pub status: VariantStatus,
    pub params: BotParameters,
    pub performance: VariantPerformance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VariantStatus {
    Testing,   // Being tested with real trades
    Promoted,  // Outperformed parent, now in production
    Retired,   // Underperformed, archived
}

/// Bot parameters that can be evolved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotParameters {
    // RSI parameters
    pub rsi_oversold: Option<f64>,  // 20-40
    pub rsi_overbought: Option<f64>, // 60-80
    pub rsi_period: Option<i32>,     // 10-20
    
    // ATR-based parameters
    pub atr_stop_multiplier: Option<f64>,  // 1.5-3.0
    pub atr_tp_multiplier: Option<f64>,    // 2.0-5.0
    pub atr_zone_multiplier: Option<f64>,  // 0.8-2.0
    
    // ADX parameters
    pub adx_threshold: Option<f64>,  // 15-30
    
    // Timeframe
    pub timeframe: Option<String>,  // H1, H4, D1
    
    // Risk parameters
    pub lot_size_multiplier: Option<f64>, // 0.8-1.2
}

impl BotParameters {
    /// Clamp all parameters to safe ranges
    pub fn clamp_to_safe_ranges(&mut self) {
        if let Some(v) = self.rsi_oversold {
            self.rsi_oversold = Some(v.clamp(20.0, 40.0));
        }
        if let Some(v) = self.rsi_overbought {
            self.rsi_overbought = Some(v.clamp(60.0, 80.0));
        }
        if let Some(v) = self.rsi_period {
            self.rsi_period = Some(v.clamp(10, 20));
        }
        if let Some(v) = self.atr_stop_multiplier {
            self.atr_stop_multiplier = Some(v.clamp(1.5, 3.0));
        }
        if let Some(v) = self.atr_tp_multiplier {
            self.atr_tp_multiplier = Some(v.clamp(2.0, 5.0));
        }
        if let Some(v) = self.atr_zone_multiplier {
            self.atr_zone_multiplier = Some(v.clamp(0.8, 2.0));
        }
        if let Some(v) = self.adx_threshold {
            self.adx_threshold = Some(v.clamp(15.0, 30.0));
        }
        if let Some(v) = self.lot_size_multiplier {
            self.lot_size_multiplier = Some(v.clamp(0.8, 1.2));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantPerformance {
    pub trades_count: i32,
    pub wins: i32,
    pub losses: i32,
    pub total_pnl: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub sharpe_ratio: f64,
}

impl Default for VariantPerformance {
    fn default() -> Self {
        Self {
            trades_count: 0,
            wins: 0,
            losses: 0,
            total_pnl: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            sharpe_ratio: 0.0,
        }
    }
}

/// Mutation strategy
#[derive(Debug, Clone)]
pub enum MutationType {
    Conservative, // Small mutations (±10-20%)
    Aggressive,   // Large mutations (±30-50%)
    Random,       // Random mix
}

pub struct EvolutionEngine {
    pool: PgPool,
}

impl EvolutionEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Run evolution cycle: select top performers, create variants
    /// Should be run weekly
    pub async fn evolve_generation(&self) -> Result<Vec<Uuid>> {
        info!("Evolution: Starting new generation");
        
        // 1. Get top 20% performers from active bots
        let parents = self.get_top_performers(0.2).await?;
        
        if parents.is_empty() {
            warn!("Evolution: No eligible parents found");
            return Ok(vec![]);
        }
        
        info!("Evolution: Selected {} parents for breeding", parents.len());
        
        // 2. Create 2-3 variants per parent
        let mut new_variant_ids = Vec::new();
        
        for parent in parents {
            let current_gen = self.get_latest_generation(parent.id).await?;
            let new_gen = current_gen + 1;
            
            // Load parent parameters
            let parent_params = self.load_bot_params(parent.id).await?;
            
            // Create conservative variant
            let conservative = self.mutate_params(&parent_params, MutationType::Conservative);
            let variant_id = self.create_variant(
                parent.id,
                format!("{} Gen{} Conservative", parent.name, new_gen),
                new_gen,
                conservative,
            ).await?;
            new_variant_ids.push(variant_id);
            
            // Create aggressive variant
            let aggressive = self.mutate_params(&parent_params, MutationType::Aggressive);
            let variant_id = self.create_variant(
                parent.id,
                format!("{} Gen{} Aggressive", parent.name, new_gen),
                new_gen,
                aggressive,
            ).await?;
            new_variant_ids.push(variant_id);
            
            // 30% chance for random variant
            if rand::random::<f64>() < 0.3 {
                let random = self.mutate_params(&parent_params, MutationType::Random);
                let variant_id = self.create_variant(
                    parent.id,
                    format!("{} Gen{} Random", parent.name, new_gen),
                    new_gen,
                    random,
                ).await?;
                new_variant_ids.push(variant_id);
            }
        }
        
        info!("Evolution: Created {} new variants", new_variant_ids.len());
        Ok(new_variant_ids)
    }
    
    /// Mutate parameters based on strategy
    fn mutate_params(&self, base: &BotParameters, mutation: MutationType) -> BotParameters {
        let mut mutated = base.clone();
        
        let mutation_range = match mutation {
            MutationType::Conservative => 0.15, // ±15%
            MutationType::Aggressive => 0.4,    // ±40%
            MutationType::Random => rand::random::<f64>() * 0.5, // 0-50%
        };
        
        // Mutate RSI parameters
        if let Some(v) = mutated.rsi_oversold {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.rsi_oversold = Some(v + delta);
        }
        
        if let Some(v) = mutated.rsi_overbought {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.rsi_overbought = Some(v + delta);
        }
        
        if let Some(v) = mutated.rsi_period {
            let delta = ((rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v as f64) as i32;
            mutated.rsi_period = Some(v + delta);
        }
        
        // Mutate ATR parameters
        if let Some(v) = mutated.atr_stop_multiplier {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.atr_stop_multiplier = Some(v + delta);
        }
        
        if let Some(v) = mutated.atr_tp_multiplier {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.atr_tp_multiplier = Some(v + delta);
        }
        
        if let Some(v) = mutated.atr_zone_multiplier {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.atr_zone_multiplier = Some(v + delta);
        }
        
        // Mutate ADX
        if let Some(v) = mutated.adx_threshold {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.adx_threshold = Some(v + delta);
        }
        
        // Mutate lot size
        if let Some(v) = mutated.lot_size_multiplier {
            let delta = (rand::random::<f64>() - 0.5) * 2.0 * mutation_range * v;
            mutated.lot_size_multiplier = Some(v + delta);
        }
        
        // Clamp to safe ranges
        mutated.clamp_to_safe_ranges();
        mutated
    }
    
    /// Create a new variant in the database
    async fn create_variant(
        &self,
        parent_bot_id: Uuid,
        variant_name: String,
        generation: i32,
        params: BotParameters,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let params_json = serde_json::to_value(&params)?;
        
        sqlx::query!(
            r#"
            INSERT INTO bot_variants 
                (id, parent_bot_id, variant_name, generation, status, params, 
                 trades_count, wins, losses, total_pnl, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, 0, 0, 0, 0.0, $7)
            "#,
            id,
            parent_bot_id,
            variant_name,
            generation,
            "testing",
            params_json,
            Utc::now().naive_utc()
        )
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// Get top performing bots (by profit factor and win rate)
    async fn get_top_performers(&self, top_pct: f64) -> Result<Vec<ParentBot>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                b.id,
                b.name,
                COUNT(CASE WHEN tc.pnl > 0 THEN 1 END) as wins,
                COUNT(CASE WHEN tc.pnl <= 0 THEN 1 END) as losses,
                COUNT(*) as total_trades,
                AVG(CASE WHEN tc.pnl > 0 THEN 1.0 ELSE 0.0 END) as win_rate,
                SUM(tc.pnl) as total_pnl
            FROM bots b
            LEFT JOIN trade_context tc ON tc.bot_id = b.id
            WHERE b.is_active = true
              AND tc.created_at > NOW() - INTERVAL '30 days'
            GROUP BY b.id, b.name
            HAVING COUNT(*) >= 20
            ORDER BY 
                AVG(CASE WHEN tc.pnl > 0 THEN 1.0 ELSE 0.0 END) DESC,
                SUM(tc.pnl) DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        // Take top percentage
        let take_count = ((rows.len() as f64 * top_pct).ceil() as usize).max(1);
        
        Ok(rows
            .into_iter()
            .take(take_count)
            .filter_map(|row| {
                Some(ParentBot {
                    id: row.id,
                    name: row.name,
                    wins: row.wins? as i32,
                    losses: row.losses? as i32,
                    total_trades: row.total_trades? as i32,
                    win_rate: row.win_rate?.to_string().parse().ok()?,
                    total_pnl: row.total_pnl?.to_string().parse().ok()?,
                })
            })
            .collect())
    }
    
    /// Promote variants that outperform their parents
    /// Run this weekly after evolution
    pub async fn promote_variants(&self) -> Result<Vec<Uuid>> {
        let ready_variants = sqlx::query!(
            r#"
            SELECT id, parent_bot_id, trades_count, win_rate, profit_factor, total_pnl
            FROM bot_variants
            WHERE status = 'testing' AND trades_count >= 50
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut promoted = Vec::new();
        
        for variant in ready_variants {
            // Get parent performance
            let parent_perf = self.get_bot_performance(variant.parent_bot_id).await?;
            
            let variant_pf = variant.profit_factor.unwrap_or(0.0);
            let variant_wr = variant.win_rate.unwrap_or(0.0);
            
            // Promote if 10% better on profit factor OR 5% better on win rate
            let pf_improvement = (variant_pf as f64 - parent_perf.profit_factor as f64) / parent_perf.profit_factor as f64;
            let wr_improvement = variant_wr as f64 - parent_perf.win_rate as f64;
            
            if pf_improvement > 0.1 || wr_improvement > 0.05 {
                self.promote_variant(variant.id).await?;
                promoted.push(variant.id);
                
                info!(
                    "Evolution: Promoted variant {} (PF: {:.2} vs {:.2}, WR: {:.2} vs {:.2})",
                    variant.id, variant_pf, parent_perf.profit_factor, variant_wr, parent_perf.win_rate
                );
            } else {
                // Retire underperformer
                self.retire_variant(variant.id).await?;
            }
        }
        
        info!("Evolution: Promoted {} variants", promoted.len());
        Ok(promoted)
    }
    
    /// Mark variant as promoted
    async fn promote_variant(&self, variant_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE bot_variants
            SET status = 'promoted', promoted_at = $2
            WHERE id = $1
            "#,
            variant_id,
            Utc::now().naive_utc()
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Mark variant as retired
    async fn retire_variant(&self, variant_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE bot_variants
            SET status = 'retired', retired_at = $2
            WHERE id = $1
            "#,
            variant_id,
            Utc::now().naive_utc()
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Get latest generation number for a bot
    async fn get_latest_generation(&self, bot_id: Uuid) -> Result<i32> {
        let result = sqlx::query!(
            r#"
            SELECT MAX(generation) as max_gen
            FROM bot_variants
            WHERE parent_bot_id = $1
            "#,
            bot_id
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(result.max_gen.unwrap_or(0))
    }
    
    /// Load bot parameters from strategy
    async fn load_bot_params(&self, bot_id: Uuid) -> Result<BotParameters> {
        // TODO: In production, query actual bot strategy parameters
        // For now, return defaults
        Ok(BotParameters {
            rsi_oversold: Some(30.0),
            rsi_overbought: Some(70.0),
            rsi_period: Some(14),
            atr_stop_multiplier: Some(2.0),
            atr_tp_multiplier: Some(3.0),
            atr_zone_multiplier: Some(1.5),
            adx_threshold: Some(20.0),
            timeframe: Some("H4".to_string()),
            lot_size_multiplier: Some(1.0),
        })
    }
    
    /// Get bot performance metrics
    async fn get_bot_performance(&self, bot_id: Uuid) -> Result<VariantPerformance> {
        let result = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as trades,
                COUNT(CASE WHEN pnl > 0 THEN 1 END) as wins,
                COUNT(CASE WHEN pnl <= 0 THEN 1 END) as losses,
                SUM(pnl) as total_pnl,
                AVG(CASE WHEN pnl > 0 THEN 1.0 ELSE 0.0 END) as win_rate
            FROM trade_context
            WHERE bot_id = $1 AND created_at > NOW() - INTERVAL '30 days'
            "#,
            bot_id
        )
        .fetch_one(&self.pool)
        .await?;
        
        let trades_count = result.trades.unwrap_or(0) as i32;
        let wins = result.wins.unwrap_or(0) as i32;
        let losses = result.losses.unwrap_or(0) as i32;
        let total_pnl = result.total_pnl.map(|v| v.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);
        let win_rate = result.win_rate.map(|v| v.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0);
        
        let profit_factor = if losses > 0 {
            // Simplified profit factor calculation
            (wins as f64 / losses as f64).max(0.0)
        } else if wins > 0 {
            wins as f64
        } else {
            0.0
        };
        
        Ok(VariantPerformance {
            trades_count,
            wins,
            losses,
            total_pnl,
            win_rate,
            profit_factor,
            sharpe_ratio: 0.0, // TODO: Calculate from returns
        })
    }
    
    /// Get all variants for a parent bot
    pub async fn get_variants_for_bot(&self, bot_id: Uuid) -> Result<Vec<BotVariant>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, parent_bot_id, variant_name, generation, status, params,
                   trades_count, wins, losses, total_pnl, win_rate, profit_factor,
                   sharpe_ratio, created_at, promoted_at, retired_at
            FROM bot_variants
            WHERE parent_bot_id = $1
            ORDER BY generation DESC, created_at DESC
            "#,
            bot_id
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let status = match row.status.as_str() {
                    "testing" => VariantStatus::Testing,
                    "promoted" => VariantStatus::Promoted,
                    "retired" => VariantStatus::Retired,
                    _ => return None,
                };
                
                let params: BotParameters = serde_json::from_value(row.params).ok()?;
                
                Some(BotVariant {
                    id: row.id,
                    parent_bot_id: row.parent_bot_id,
                    variant_name: row.variant_name,
                    generation: row.generation,
                    status,
                    params,
                    performance: VariantPerformance {
                        trades_count: row.trades_count,
                        wins: row.wins,
                        losses: row.losses,
                        total_pnl: row.total_pnl as f64,
                        win_rate: row.win_rate.unwrap_or(0.0) as f64,
                        profit_factor: row.profit_factor.unwrap_or(0.0) as f64,
                        sharpe_ratio: row.sharpe_ratio.unwrap_or(0.0) as f64,
                    },
                    created_at: DateTime::from_naive_utc_and_offset(row.created_at, Utc),
                })
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
struct ParentBot {
    id: Uuid,
    name: String,
    wins: i32,
    losses: i32,
    total_trades: i32,
    win_rate: f64,
    total_pnl: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parameter_mutation() {
        let engine = EvolutionEngine { pool: todo!() };
        
        let base = BotParameters {
            rsi_oversold: Some(30.0),
            rsi_overbought: Some(70.0),
            rsi_period: Some(14),
            atr_stop_multiplier: Some(2.0),
            atr_tp_multiplier: Some(3.0),
            atr_zone_multiplier: Some(1.5),
            adx_threshold: Some(20.0),
            timeframe: Some("H4".to_string()),
            lot_size_multiplier: Some(1.0),
        };
        
        let conservative = engine.mutate_params(&base, MutationType::Conservative);
        let aggressive = engine.mutate_params(&base, MutationType::Aggressive);
        
        // Conservative should be close to original
        if let (Some(orig), Some(cons)) = (base.rsi_oversold, conservative.rsi_oversold) {
            let diff_pct = ((cons - orig).abs() / orig) * 100.0;
            assert!(diff_pct < 20.0, "Conservative mutation too large");
        }
        
        // Both should be within safe ranges
        assert!(conservative.rsi_oversold.unwrap() >= 20.0);
        assert!(conservative.rsi_oversold.unwrap() <= 40.0);
        assert!(aggressive.rsi_oversold.unwrap() >= 20.0);
        assert!(aggressive.rsi_oversold.unwrap() <= 40.0);
    }
}
