//! Trailing Stop Manager - ML-Powered Dynamic Exit Strategy
//!
//! Uses ML systems to determine:
//! - When to activate trailing stops (regime-based)
//! - How tight to trail (correlation + volatility aware)
//! - When to extend TP targets (momentum-based)
//! - When to tighten stops preemptively (regime transitions)

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, types::BigDecimal};
use std::str::FromStr;
use tracing::{info, warn};
use uuid::Uuid;

use crate::mrate::predictor::RegimePredictor;
use crate::analytics::correlation::CorrelationEngine;

/// Configuration for trailing stop behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailingStopConfig {
    /// Base trailing distance in ATR multiples
    pub base_trail_atr: f64,
    
    /// Minimum profit (in ATR) before activating trail
    pub min_profit_to_trail_atr: f64,
    
    /// TP extension trigger: extend if this % to original TP
    pub tp_extension_threshold: f64,
    
    /// TP extension multiplier: new_tp = old_tp * this
    pub tp_extension_multiplier: f64,
    
    /// Regime-based adjustments
    pub trend_regime_trail_multiplier: f64,    // Looser trail in trends (1.5x)
    pub panic_regime_trail_multiplier: f64,    // Tighter trail in panic (0.6x)
    
    /// Correlation-based adjustments
    pub high_correlation_multiplier: f64,      // Tighter when corr >0.7 (0.7x)
    
    /// Sentiment-based adjustments
    pub extreme_fear_tp_multiplier: f64,       // Extend TP in extreme fear (1.3x)
}

impl Default for TrailingStopConfig {
    fn default() -> Self {
        Self {
            base_trail_atr: 2.0,
            min_profit_to_trail_atr: 0.5,
            tp_extension_threshold: 0.7,      // Extend if 70% to TP
            tp_extension_multiplier: 1.4,     // Extend by 40%
            trend_regime_trail_multiplier: 1.5,
            panic_regime_trail_multiplier: 0.6,
            high_correlation_multiplier: 0.7,
            extreme_fear_tp_multiplier: 1.3,
        }
    }
}

/// State of a trailing stop for an open position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailingStopState {
    pub position_id: Uuid,
    pub instrument: String,
    pub direction: String,  // "long" or "short"
    
    pub entry_price: f64,
    pub original_stop: f64,
    pub original_tp: f64,
    pub current_stop: f64,
    pub current_tp: f64,
    
    pub atr: f64,
    pub trail_distance_atr: f64,
    
    pub trailing_active: bool,
    pub tp_extended: bool,
    pub highest_price: f64,  // For longs
    pub lowest_price: f64,   // For shorts
    
    pub regime_adjustment: String,
    pub correlation_adjustment: f64,
    pub sentiment_adjustment: f64,
    
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailingStopDecision {
    pub should_update_stop: bool,
    pub new_stop: Option<f64>,
    pub should_extend_tp: bool,
    pub new_tp: Option<f64>,
    pub reasoning: String,
}

pub struct TrailingStopManager {
    pool: PgPool,
    config: TrailingStopConfig,
    regime_predictor: RegimePredictor,
    correlation_engine: CorrelationEngine,
}

impl TrailingStopManager {
    pub fn new(pool: PgPool, config: Option<TrailingStopConfig>) -> Self {
        let regime_predictor = RegimePredictor::new(pool.clone());
        let correlation_engine = CorrelationEngine::new(pool.clone());
        
        Self {
            pool,
            config: config.unwrap_or_default(),
            regime_predictor,
            correlation_engine,
        }
    }
    
    /// Initialize trailing stop state for a new position
    pub async fn initialize_position(
        &self,
        position_id: Uuid,
        instrument: String,
        direction: String,
        entry_price: f64,
        stop_loss: f64,
        take_profit: f64,
        atr: f64,
    ) -> Result<TrailingStopState> {
        let instrument_display = instrument.clone();
        let state = TrailingStopState {
            position_id,
            instrument,
            direction: direction.clone(),
            entry_price,
            original_stop: stop_loss,
            original_tp: take_profit,
            current_stop: stop_loss,
            current_tp: take_profit,
            atr,
            trail_distance_atr: self.config.base_trail_atr,
            trailing_active: false,
            tp_extended: false,
            highest_price: entry_price,
            lowest_price: entry_price,
            regime_adjustment: "none".to_string(),
            correlation_adjustment: 1.0,
            sentiment_adjustment: 1.0,
            created_at: Utc::now(),
            last_updated: Utc::now(),
        };
        
        // Save to database
        self.save_state(&state).await?;
        
        info!(
            "Trailing stop initialized for {} {} @ {} (SL: {}, TP: {}, ATR: {})",
            direction.to_uppercase(), instrument_display, entry_price, stop_loss, take_profit, atr
        );
        
        Ok(state)
    }
    
    /// Update trailing stop based on current price and ML signals
    pub async fn update_trailing_stop(
        &self,
        position_id: Uuid,
        current_price: f64,
    ) -> Result<TrailingStopDecision> {
        // Load current state
        let mut state = self.load_state(position_id).await?;
        
        // Update high/low watermarks
        if state.direction == "long" {
            state.highest_price = state.highest_price.max(current_price);
        } else {
            state.lowest_price = state.lowest_price.min(current_price);
        }
        
        // Calculate current profit in ATR
        let profit_atr = if state.direction == "long" {
            (current_price - state.entry_price) / state.atr
        } else {
            (state.entry_price - current_price) / state.atr
        };
        
        // Get ML adjustments
        let ml_adjustments = self.get_ml_adjustments(&state).await?;
        
        // Calculate adjusted trail distance
        let adjusted_trail_atr = self.config.base_trail_atr 
            * ml_adjustments.regime_multiplier
            * ml_adjustments.correlation_multiplier
            * ml_adjustments.sentiment_multiplier;
        
        state.trail_distance_atr = adjusted_trail_atr;
        state.regime_adjustment = ml_adjustments.regime_reason.clone();
        state.correlation_adjustment = ml_adjustments.correlation_multiplier;
        state.sentiment_adjustment = ml_adjustments.sentiment_multiplier;
        
        // Check if trailing should activate
        if !state.trailing_active && profit_atr >= self.config.min_profit_to_trail_atr {
            state.trailing_active = true;
            info!(
                "Trailing stop ACTIVATED for {} (profit: {:.2} ATR)",
                position_id, profit_atr
            );
        }
        
        let mut decision = TrailingStopDecision {
            should_update_stop: false,
            new_stop: None,
            should_extend_tp: false,
            new_tp: None,
            reasoning: String::new(),
        };
        
        // Update stop if trailing is active
        if state.trailing_active {
            let new_stop = if state.direction == "long" {
                state.highest_price - (adjusted_trail_atr * state.atr)
            } else {
                state.lowest_price + (adjusted_trail_atr * state.atr)
            };
            
            // Only move stop in favorable direction
            let should_update = if state.direction == "long" {
                new_stop > state.current_stop
            } else {
                new_stop < state.current_stop
            };
            
            if should_update {
                decision.should_update_stop = true;
                decision.new_stop = Some(new_stop);
                decision.reasoning.push_str(&format!(
                    "Trail stop moved to {} ({:.2} ATR from high/low). ",
                    new_stop, adjusted_trail_atr
                ));
                state.current_stop = new_stop;
            }
        }
        
        // Check for TP extension
        if !state.tp_extended && profit_atr > 0.0 {
            let progress_to_tp = if state.direction == "long" {
                (current_price - state.entry_price) / (state.current_tp - state.entry_price)
            } else {
                (state.entry_price - current_price) / (state.entry_price - state.current_tp)
            };
            
            if progress_to_tp >= self.config.tp_extension_threshold {
                let tp_multiplier = self.config.tp_extension_multiplier * ml_adjustments.sentiment_multiplier;
                
                let new_tp = if state.direction == "long" {
                    state.entry_price + (state.original_tp - state.entry_price) * tp_multiplier
                } else {
                    state.entry_price - (state.entry_price - state.original_tp) * tp_multiplier
                };
                
                decision.should_extend_tp = true;
                decision.new_tp = Some(new_tp);
                decision.reasoning.push_str(&format!(
                    "Extended TP from {} to {} ({:.0}% progress, {:.1}x multiplier). ",
                    state.current_tp, new_tp, progress_to_tp * 100.0, tp_multiplier
                ));
                
                state.current_tp = new_tp;
                state.tp_extended = true;
            }
        }
        
        // Add ML reasoning
        decision.reasoning.push_str(&format!(
            "ML: {} | Corr: {:.2}x | Sentiment: {:.2}x",
            ml_adjustments.regime_reason,
            ml_adjustments.correlation_multiplier,
            ml_adjustments.sentiment_multiplier
        ));
        
        // Save updated state
        state.last_updated = Utc::now();
        self.save_state(&state).await?;
        
        Ok(decision)
    }
    
    /// Get ML-based adjustments for trailing behavior
    async fn get_ml_adjustments(&self, state: &TrailingStopState) -> Result<MLAdjustments> {
        let mut adjustments = MLAdjustments {
            regime_multiplier: 1.0,
            regime_reason: "Neutral".to_string(),
            correlation_multiplier: 1.0,
            sentiment_multiplier: 1.0,
        };
        
        // 1. Regime Prediction Adjustment - Skip for now (requires MRATE integration)
        // TODO: Integrate with MRATE system to get current regime state and predictions
        
        // 2. Correlation Adjustment
        // Note: portfolio_correlation_score expects open positions list - for now use simplified check
        if let Ok(score) = self.correlation_engine.get_correlation(&state.instrument, &state.instrument).await {
            if score > 0.7 {
                adjustments.correlation_multiplier = self.config.high_correlation_multiplier;
            }
        }
        
        // 3. Sentiment Adjustment (for TP extension)
        if let Ok(Some(inputs)) = self.get_latest_mrate_inputs().await {
            if let Some(fg_index) = inputs.fear_greed_index {
                if fg_index < 20 && state.direction == "long" {
                    // Extreme fear + long = extend TP more (contrarian)
                    adjustments.sentiment_multiplier = self.config.extreme_fear_tp_multiplier;
                } else if fg_index > 80 && state.direction == "short" {
                    // Extreme greed + short = extend TP more
                    adjustments.sentiment_multiplier = self.config.extreme_fear_tp_multiplier;
                }
            }
        }
        
        Ok(adjustments)
    }
    
    /// Save state to database
    async fn save_state(&self, state: &TrailingStopState) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO trailing_stop_state 
                (position_id, instrument, direction, entry_price, original_stop, original_tp,
                 current_stop, current_tp, atr, trail_distance_atr, trailing_active, tp_extended,
                 highest_price, lowest_price, regime_adjustment, correlation_adjustment,
                 sentiment_adjustment, created_at, last_updated)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ON CONFLICT (position_id) DO UPDATE SET
                current_stop = $7,
                current_tp = $8,
                trail_distance_atr = $10,
                trailing_active = $11,
                tp_extended = $12,
                highest_price = $13,
                lowest_price = $14,
                regime_adjustment = $15,
                correlation_adjustment = $16,
                sentiment_adjustment = $17,
                last_updated = $19
            "#,
            state.position_id,
            state.instrument,
            state.direction,
            BigDecimal::from_str(&state.entry_price.to_string()).unwrap(),
            BigDecimal::from_str(&state.original_stop.to_string()).unwrap(),
            BigDecimal::from_str(&state.original_tp.to_string()).unwrap(),
            BigDecimal::from_str(&state.current_stop.to_string()).unwrap(),
            BigDecimal::from_str(&state.current_tp.to_string()).unwrap(),
            BigDecimal::from_str(&state.atr.to_string()).unwrap(),
            BigDecimal::from_str(&state.trail_distance_atr.to_string()).unwrap(),
            state.trailing_active,
            state.tp_extended,
            BigDecimal::from_str(&state.highest_price.to_string()).unwrap(),
            BigDecimal::from_str(&state.lowest_price.to_string()).unwrap(),
            state.regime_adjustment,
            BigDecimal::from_str(&state.correlation_adjustment.to_string()).unwrap(),
            BigDecimal::from_str(&state.sentiment_adjustment.to_string()).unwrap(),
            state.created_at,
            state.last_updated,
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Load state from database
    async fn load_state(&self, position_id: Uuid) -> Result<TrailingStopState> {
        let row = sqlx::query!(
            r#"
            SELECT * FROM trailing_stop_state
            WHERE position_id = $1
            "#,
            position_id
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(TrailingStopState {
            position_id: row.position_id,
            instrument: row.instrument,
            direction: row.direction,
            entry_price: row.entry_price.to_string().parse().unwrap(),
            original_stop: row.original_stop.to_string().parse().unwrap(),
            original_tp: row.original_tp.to_string().parse().unwrap(),
            current_stop: row.current_stop.to_string().parse().unwrap(),
            current_tp: row.current_tp.to_string().parse().unwrap(),
            atr: row.atr.to_string().parse().unwrap(),
            trail_distance_atr: row.trail_distance_atr.to_string().parse().unwrap(),
            trailing_active: row.trailing_active,
            tp_extended: row.tp_extended,
            highest_price: row.highest_price.to_string().parse().unwrap(),
            lowest_price: row.lowest_price.to_string().parse().unwrap(),
            regime_adjustment: row.regime_adjustment,
            correlation_adjustment: row.correlation_adjustment.to_string().parse().unwrap(),
            sentiment_adjustment: row.sentiment_adjustment.to_string().parse().unwrap(),
            created_at: row.created_at,
            last_updated: row.last_updated,
        })
    }
    
    /// Get all active trailing stops
    pub async fn get_active_trails(&self) -> Result<Vec<TrailingStopState>> {
        let rows = sqlx::query!(
            r#"
            SELECT * FROM trailing_stop_state
            WHERE trailing_active = true
            ORDER BY last_updated DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows
            .into_iter()
            .map(|row| TrailingStopState {
                position_id: row.position_id,
                instrument: row.instrument,
                direction: row.direction,
                entry_price: row.entry_price.to_string().parse().unwrap(),
                original_stop: row.original_stop.to_string().parse().unwrap(),
                original_tp: row.original_tp.to_string().parse().unwrap(),
                current_stop: row.current_stop.to_string().parse().unwrap(),
                current_tp: row.current_tp.to_string().parse().unwrap(),
                atr: row.atr.to_string().parse().unwrap(),
                trail_distance_atr: row.trail_distance_atr.to_string().parse().unwrap(),
                trailing_active: row.trailing_active,
                tp_extended: row.tp_extended,
                highest_price: row.highest_price.to_string().parse().unwrap(),
                lowest_price: row.lowest_price.to_string().parse().unwrap(),
                regime_adjustment: row.regime_adjustment,
                correlation_adjustment: row.correlation_adjustment.to_string().parse().unwrap(),
                sentiment_adjustment: row.sentiment_adjustment.to_string().parse().unwrap(),
                created_at: row.created_at,
                last_updated: row.last_updated,
            })
            .collect())
    }
    
    /// Clean up state when position closes
    pub async fn remove_position(&self, position_id: Uuid) -> Result<()> {
        sqlx::query!(
            "DELETE FROM trailing_stop_state WHERE position_id = $1",
            position_id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Get latest MRATE inputs for sentiment
    async fn get_latest_mrate_inputs(&self) -> Result<Option<MrateInputs>> {
        let row = sqlx::query!(
            r#"
            SELECT fear_greed_index, reddit_crypto as reddit_crypto_sentiment, reddit_btc as reddit_btc_sentiment
            FROM sentiment_signals
            ORDER BY timestamp DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| MrateInputs {
            fear_greed_index: r.fear_greed_index.map(|f| f as i32),
            reddit_crypto_sentiment: r.reddit_crypto_sentiment.map(|f| f as f64),
            reddit_btc_sentiment: r.reddit_btc_sentiment.map(|f| f as f64),
        }))
    }
}

#[derive(Debug, Clone)]
struct MLAdjustments {
    regime_multiplier: f64,
    regime_reason: String,
    correlation_multiplier: f64,
    sentiment_multiplier: f64,
}

#[derive(Debug, Clone)]
struct MrateInputs {
    fear_greed_index: Option<i32>,
    reddit_crypto_sentiment: Option<f64>,
    reddit_btc_sentiment: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trail_distance_calculation() {
        let config = TrailingStopConfig::default();
        
        // Base: 2.0 ATR
        // Trend regime: 1.5x = 3.0 ATR (looser)
        // Panic regime: 0.6x = 1.2 ATR (tighter)
        // High correlation: 0.7x = 1.4 ATR (tighter)
        
        let base = config.base_trail_atr;
        assert_eq!(base, 2.0);
        
        let trend_trail = base * config.trend_regime_trail_multiplier;
        assert_eq!(trend_trail, 3.0);
        
        let panic_trail = base * config.panic_regime_trail_multiplier;
        assert_eq!(panic_trail, 1.2);
    }
}
