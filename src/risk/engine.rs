//! Portfolio Risk Engine - The kill switch and coordinator
//!
//! This is the heart of the risk management system.
//! It makes life-or-death decisions for the trading account.

use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::risk::models::{AccountState, RiskDecision, RiskLimits, RiskStatus, OpenPosition};
use crate::risk::tracker::PositionTracker;
use crate::risk::correlation::CorrelationEngine;

/// The Portfolio Risk Engine - guards the account from catastrophic loss
pub struct PortfolioRiskEngine {
    /// Risk limit configuration (mutable for UI updates)
    limits: Arc<RwLock<RiskLimits>>,
    /// Current account state (shared, mutable)
    account_state: Arc<RwLock<AccountState>>,
    /// Position tracker (shared, mutable)
    position_tracker: Arc<RwLock<PositionTracker>>,
    /// Correlation engine for strategy diversification
    correlation_engine: CorrelationEngine,
}

impl PortfolioRiskEngine {
    /// Create a new risk engine with default limits
    pub fn new(starting_equity: f64) -> Self {
        Self {
            limits: Arc::new(RwLock::new(RiskLimits::default())),
            account_state: Arc::new(RwLock::new(AccountState::new(starting_equity))),
            position_tracker: Arc::new(RwLock::new(PositionTracker::new())),
            correlation_engine: CorrelationEngine::new(),
        }
    }
    
    /// Create with custom risk limits
    pub fn with_limits(starting_equity: f64, limits: RiskLimits) -> Self {
        Self {
            limits: Arc::new(RwLock::new(limits)),
            account_state: Arc::new(RwLock::new(AccountState::new(starting_equity))),
            position_tracker: Arc::new(RwLock::new(PositionTracker::new())),
            correlation_engine: CorrelationEngine::new(),
        }
    }
    
    /// Get current risk limits
    pub async fn get_limits(&self) -> RiskLimits {
        self.limits.read().await.clone()
    }
    
    /// Update risk limits (for UI control)
    pub async fn update_limits(
        &self,
        max_daily_loss_pct: Option<f64>,
        max_weekly_loss_pct: Option<f64>,
        max_drawdown_pct: Option<f64>,
        max_consecutive_losses: Option<u32>,
        max_total_exposure_multiplier: Option<f64>,
        max_per_asset_multiplier: Option<f64>,
        max_per_strategy_multiplier: Option<f64>,
    ) {
        let mut limits = self.limits.write().await;
        
        if let Some(v) = max_daily_loss_pct {
            limits.max_daily_loss_pct = v;
        }
        if let Some(v) = max_weekly_loss_pct {
            limits.max_weekly_loss_pct = v;
        }
        if let Some(v) = max_drawdown_pct {
            limits.max_drawdown_pct = v;
        }
        if let Some(v) = max_consecutive_losses {
            limits.max_consecutive_losses = v;
        }
        if let Some(v) = max_total_exposure_multiplier {
            limits.max_total_exposure_multiplier = v;
        }
        if let Some(v) = max_per_asset_multiplier {
            limits.max_per_asset_multiplier = v;
        }
        if let Some(v) = max_per_strategy_multiplier {
            limits.max_per_strategy_multiplier = v;
        }
    }
    
    /// Get a clone of the account state Arc for sharing
    pub fn account_state(&self) -> Arc<RwLock<AccountState>> {
        self.account_state.clone()
    }
    
    /// Get a clone of the position tracker Arc for sharing
    pub fn position_tracker(&self) -> Arc<RwLock<PositionTracker>> {
        self.position_tracker.clone()
    }
    
    /// THE BIG ONE: Check if account-level risk allows trading
    /// 
    /// This is called BEFORE every trade execution
    pub async fn check_account_risk(&self) -> RiskDecision {
        let limits = self.limits.read().await;
        let mut account = self.account_state.write().await;
        
        // Handle daily/weekly resets
        if account.needs_daily_reset() {
            tracing::info!("🔄 Daily risk stats reset");
            account.reset_daily();
        }
        if account.needs_weekly_reset() {
            tracing::info!("🔄 Weekly risk stats reset");
            account.reset_weekly();
        }
        
        // If kill switch is already active, emergency shutdown
        if account.kill_switch_active {
            return RiskDecision::EmergencyShutdown;
        }
        
        // Calculate current metrics
        let drawdown_pct = account.current_drawdown_pct();
        let daily_loss_pct = account.daily_loss_pct();
        let weekly_loss_pct = account.weekly_loss_pct();
        let consecutive_losses = account.consecutive_losses;
        
        // 🚨 EMERGENCY SHUTDOWN CONDITIONS 🚨
        
        // 1. Max drawdown breached
        if drawdown_pct >= limits.max_drawdown_pct {
            tracing::error!(
                "🚨 EMERGENCY SHUTDOWN: Max drawdown breached ({:.2}% >= {:.2}%)",
                drawdown_pct, limits.max_drawdown_pct
            );
            account.kill_switch_active = true;
            return RiskDecision::EmergencyShutdown;
        }
        
        // 2. Daily loss limit breached
        if daily_loss_pct >= limits.max_daily_loss_pct {
            tracing::error!(
                "🚨 EMERGENCY SHUTDOWN: Daily loss limit breached ({:.2}% >= {:.2}%)",
                daily_loss_pct, limits.max_daily_loss_pct
            );
            account.kill_switch_active = true;
            return RiskDecision::EmergencyShutdown;
        }
        
        // 3. Weekly loss limit breached
        if weekly_loss_pct >= limits.max_weekly_loss_pct {
            tracing::error!(
                "🚨 EMERGENCY SHUTDOWN: Weekly loss limit breached ({:.2}% >= {:.2}%)",
                weekly_loss_pct, limits.max_weekly_loss_pct
            );
            account.kill_switch_active = true;
            return RiskDecision::EmergencyShutdown;
        }
        
        // 4. Too many consecutive losses
        if consecutive_losses >= limits.max_consecutive_losses {
            tracing::error!(
                "🚨 EMERGENCY SHUTDOWN: Max consecutive losses ({} >= {})",
                consecutive_losses, limits.max_consecutive_losses
            );
            account.kill_switch_active = true;
            return RiskDecision::EmergencyShutdown;
        }
        
        // ⚠️ WARNING ZONE - Reduce position sizing
        
        // Approaching drawdown limit (80% of max)
        if drawdown_pct >= limits.max_drawdown_pct * 0.8 {
            let multiplier = 0.5;  // Trade at half size
            tracing::warn!(
                "⚠️ High drawdown ({:.2}%), reducing position size to {}%",
                drawdown_pct, multiplier * 100.0
            );
            return RiskDecision::ReducePositionSize(multiplier);
        }
        
        // Approaching daily loss limit (75% of max)
        if daily_loss_pct >= limits.max_daily_loss_pct * 0.75 {
            let multiplier = 0.6;  // Trade at 60% size
            tracing::warn!(
                "⚠️ High daily loss ({:.2}%), reducing position size to {}%",
                daily_loss_pct, multiplier * 100.0
            );
            return RiskDecision::ReducePositionSize(multiplier);
        }
        
        // Approaching weekly loss limit (75% of max)
        if weekly_loss_pct >= limits.max_weekly_loss_pct * 0.75 {
            let multiplier = 0.7;  // Trade at 70% size
            tracing::warn!(
                "⚠️ High weekly loss ({:.2}%), reducing position size to {}%",
                weekly_loss_pct, multiplier * 100.0
            );
            return RiskDecision::ReducePositionSize(multiplier);
        }
        
        // Multiple consecutive losses (half of max)
        if consecutive_losses >= limits.max_consecutive_losses / 2 {
            let multiplier = 0.75;  // Trade at 75% size
            tracing::warn!(
                "⚠️ {} consecutive losses, reducing position size to {}%",
                consecutive_losses, multiplier * 100.0
            );
            return RiskDecision::ReducePositionSize(multiplier);
        }
        
        // All clear - full throttle
        RiskDecision::AllowTrading
    }
    
    /// Check if a new position would violate exposure limits
    pub async fn check_exposure_limits(
        &self,
        proposed_notional: f64,
        instrument: &str,
        strategy_id: Uuid,
    ) -> RiskDecision {
        let limits = self.limits.read().await;
        let account = self.account_state.read().await;
        let tracker = self.position_tracker.read().await;
        
        let equity = account.equity;
        let current_total_exposure = tracker.total_exposure();
        let new_total_exposure = current_total_exposure + proposed_notional;
        
        // Check total exposure limit
        let max_total = equity * limits.max_total_exposure_multiplier;
        if new_total_exposure > max_total {
            tracing::warn!(
                "⛔ Total exposure limit: ${:.2} + ${:.2} = ${:.2} > ${:.2} ({}x equity)",
                current_total_exposure, proposed_notional, new_total_exposure, max_total,
                limits.max_total_exposure_multiplier
            );
            return RiskDecision::BlockNewTrades;
        }
        
        // Check per-asset exposure limit
        let asset = instrument.split('_').next().unwrap_or(instrument);
        let current_asset_exposure = tracker.exposure_by_asset(asset);
        let new_asset_exposure = current_asset_exposure + proposed_notional;
        let max_asset = equity * limits.max_per_asset_multiplier;
        
        if new_asset_exposure > max_asset {
            tracing::warn!(
                "⛔ Asset ({}) exposure limit: ${:.2} + ${:.2} = ${:.2} > ${:.2} ({}x equity)",
                asset, current_asset_exposure, proposed_notional, new_asset_exposure, max_asset,
                limits.max_per_asset_multiplier
            );
            return RiskDecision::BlockNewTrades;
        }
        
        // Check per-strategy exposure limit
        let current_strategy_exposure = tracker.exposure_by_strategy(strategy_id);
        let new_strategy_exposure = current_strategy_exposure + proposed_notional;
        let max_strategy = equity * limits.max_per_strategy_multiplier;
        
        if new_strategy_exposure > max_strategy {
            tracing::warn!(
                "⛔ Strategy exposure limit: ${:.2} + ${:.2} = ${:.2} > ${:.2} ({}x equity)",
                current_strategy_exposure, proposed_notional, new_strategy_exposure, max_strategy,
                limits.max_per_strategy_multiplier
            );
            return RiskDecision::BlockNewTrades;
        }
        
        RiskDecision::AllowTrading
    }
    
    /// Record a new position
    pub async fn record_position(&self, position: OpenPosition) {
        let mut tracker = self.position_tracker.write().await;
        tracker.add_position(position);
    }
    
    /// Record a closed position
    pub async fn record_position_closed(&self, position_id: Uuid, pnl: f64) {
        let mut tracker = self.position_tracker.write().await;
        tracker.remove_position(position_id);
        
        // Update account state
        let mut account = self.account_state.write().await;
        account.record_trade(pnl);
        
        tracing::info!(
            "💰 Trade closed: P&L ${:.2}, Daily: ${:.2}, Consecutive losses: {}",
            pnl, account.daily_pnl, account.consecutive_losses
        );
    }
    
    /// Record a closed position by instrument (when we don't have exact position_id)
    /// Uses FIFO - closes the oldest position for this instrument/bot
    pub async fn record_position_closed_by_instrument(
        &self,
        instrument: &str,
        bot_id: Uuid,
        pnl: f64,
    ) {
        let mut tracker = self.position_tracker.write().await;
        let removed = tracker.remove_position_by_instrument(instrument, bot_id);
        
        if removed.is_none() {
            tracing::warn!(
                "No open position found for {} (bot {}), but recording P&L anyway",
                instrument, bot_id
            );
        }
        
        // Update account state regardless of whether we found the position
        let mut account = self.account_state.write().await;
        account.record_trade(pnl);
        
        tracing::info!(
            "💰 Trade closed: P&L ${:.2}, Daily: ${:.2}, Consecutive losses: {}",
            pnl, account.daily_pnl, account.consecutive_losses
        );
    }
    
    /// Get current risk status for monitoring/dashboard
    pub async fn get_risk_status(&self) -> RiskStatus {
        let limits = self.limits.read().await;
        let account = self.account_state.read().await;
        let tracker = self.position_tracker.read().await;
        
        let current_decision = {
            // Re-evaluate risk decision without mutating state
            let drawdown = account.current_drawdown_pct();
            let daily_loss = account.daily_loss_pct();
            let weekly_loss = account.weekly_loss_pct();
            
            if account.kill_switch_active {
                RiskDecision::EmergencyShutdown
            } else if drawdown >= limits.max_drawdown_pct 
                || daily_loss >= limits.max_daily_loss_pct
                || weekly_loss >= limits.max_weekly_loss_pct
                || account.consecutive_losses >= limits.max_consecutive_losses {
                RiskDecision::EmergencyShutdown
            } else if drawdown >= limits.max_drawdown_pct * 0.8 {
                RiskDecision::ReducePositionSize(0.5)
            } else if daily_loss >= limits.max_daily_loss_pct * 0.75 {
                RiskDecision::ReducePositionSize(0.6)
            } else {
                RiskDecision::AllowTrading
            }
        };
        
        let total_exposure = tracker.total_exposure();
        let exposure_pct = if account.equity > 0.0 {
            (total_exposure / account.equity) * 100.0
        } else {
            0.0
        };
        
        let mut warnings = Vec::new();
        
        let drawdown = account.current_drawdown_pct();
        if drawdown > limits.max_drawdown_pct * 0.5 {
            warnings.push(format!("Drawdown at {:.1}% (limit: {:.1}%)", drawdown, limits.max_drawdown_pct));
        }
        
        if account.consecutive_losses > 3 {
            warnings.push(format!("{} consecutive losses", account.consecutive_losses));
        }
        
        if account.daily_loss_pct() > limits.max_daily_loss_pct * 0.5 {
            warnings.push(format!("Daily loss: {:.1}%", account.daily_loss_pct()));
        }
        
        RiskStatus {
            account_state: account.clone(),
            current_decision,
            drawdown_pct: drawdown,
            daily_loss_pct: account.daily_loss_pct(),
            weekly_loss_pct: account.weekly_loss_pct(),
            total_exposure,
            exposure_pct_of_equity: exposure_pct,
            warnings,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Manual override to reset kill switch (use with caution!)
    pub async fn reset_kill_switch(&self) {
        let mut account = self.account_state.write().await;
        if account.kill_switch_active {
            tracing::warn!("🔓 Kill switch manually reset");
            account.kill_switch_active = false;
        }
    }
    
    /// Update account equity (called periodically from broker data)
    pub async fn update_equity(&self, new_equity: f64) {
        let mut account = self.account_state.write().await;
        account.update_equity(new_equity);
    }
    
    /// Check if a strategy is correlated with open positions (Phase 3)
    /// 
    /// Returns true if correlation > 0.7 with any currently open strategy
    pub async fn check_strategy_correlation(
        &self,
        pool: &sqlx::PgPool,
        strategy_id: Uuid,
    ) -> Result<bool, String> {
        let tracker = self.position_tracker.read().await;
        let open_positions = tracker.get_all_positions();
        
        // Get unique strategy IDs from open positions
        let mut open_strategy_ids: Vec<Uuid> = open_positions
            .iter()
            .map(|p| p.strategy_id)
            .collect();
        open_strategy_ids.sort();
        open_strategy_ids.dedup();
        
        self.correlation_engine.is_correlated(
            pool,
            strategy_id,
            &open_strategy_ids,
        ).await
    }
    
    /// Calculate correlation between two strategies
    pub async fn calculate_correlation(
        &self,
        pool: &sqlx::PgPool,
        strategy_a: Uuid,
        strategy_b: Uuid,
    ) -> Result<f64, String> {
        self.correlation_engine.calculate_strategy_correlation(
            pool,
            strategy_a,
            strategy_b,
        ).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_kill_switch_on_max_drawdown() {
        let engine = PortfolioRiskEngine::new(10000.0);
        
        // Simulate 15% drawdown (peak 10k, now 8.5k)
        engine.update_equity(8500.0).await;
        
        let decision = engine.check_account_risk().await;
        assert_eq!(decision, RiskDecision::EmergencyShutdown);
    }
    
    #[tokio::test]
    async fn test_position_size_reduction() {
        let engine = PortfolioRiskEngine::new(10000.0);
        
        // Simulate 9% drawdown (just below 10% limit, but above 80% threshold)
        engine.update_equity(9100.0).await;
        
        let decision = engine.check_account_risk().await;
        match decision {
            RiskDecision::ReducePositionSize(mult) => {
                assert_eq!(mult, 0.5);
            }
            _ => panic!("Expected ReducePositionSize"),
        }
    }
    
    #[tokio::test]
    async fn test_exposure_limits() {
        let engine = PortfolioRiskEngine::new(10000.0);
        let bot_id = Uuid::new_v4();
        let strategy_id = Uuid::new_v4();
        
        // Try to open a massive position (6x equity = 60k notional)
        let decision = engine.check_exposure_limits(
            60000.0,
            "XAU_USD",
            strategy_id,
        ).await;
        
        assert_eq!(decision, RiskDecision::BlockNewTrades);
    }
}
