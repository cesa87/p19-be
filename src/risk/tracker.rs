//! Position tracking for exposure management
//!
//! Tracks all open positions across all bots to enforce exposure limits

use std::collections::HashMap;
use uuid::Uuid;
use crate::risk::models::OpenPosition;

/// Tracks all open positions for exposure monitoring
#[derive(Debug, Clone)]
pub struct PositionTracker {
    /// Map of position_id -> OpenPosition
    open_positions: HashMap<Uuid, OpenPosition>,
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            open_positions: HashMap::new(),
        }
    }
    
    /// Add a new position to tracking
    pub fn add_position(&mut self, position: OpenPosition) {
        tracing::info!(
            "📊 Tracking new position: {} {} @ {:.2}, notional: ${:.2}",
            position.instrument,
            position.direction,
            position.entry_price,
            position.notional_value
        );
        self.open_positions.insert(position.position_id, position);
    }
    
    /// Remove a position from tracking (when closed)
    pub fn remove_position(&mut self, position_id: Uuid) -> Option<OpenPosition> {
        let removed = self.open_positions.remove(&position_id);
        if let Some(ref pos) = removed {
            tracing::info!(
                "📊 Position closed: {} {} @ {:.2}",
                pos.instrument,
                pos.direction,
                pos.entry_price
            );
        }
        removed
    }
    
    /// Get total notional exposure across all positions
    pub fn total_exposure(&self) -> f64 {
        self.open_positions.values()
            .map(|p| p.notional_value)
            .sum()
    }
    
    /// Get exposure for a specific asset (e.g., "XAU" for gold, "BTC" for bitcoin)
    pub fn exposure_by_asset(&self, asset: &str) -> f64 {
        self.open_positions.values()
            .filter(|p| p.instrument.contains(asset))
            .map(|p| p.notional_value)
            .sum()
    }
    
    /// Get exposure for a specific strategy
    pub fn exposure_by_strategy(&self, strategy_id: Uuid) -> f64 {
        self.open_positions.values()
            .filter(|p| p.strategy_id == strategy_id)
            .map(|p| p.notional_value)
            .sum()
    }
    
    /// Get exposure for a specific bot
    pub fn exposure_by_bot(&self, bot_id: Uuid) -> f64 {
        self.open_positions.values()
            .filter(|p| p.bot_id == bot_id)
            .map(|p| p.notional_value)
            .sum()
    }
    
    /// Get all open positions
    pub fn get_all_positions(&self) -> Vec<OpenPosition> {
        self.open_positions.values().cloned().collect()
    }
    
    /// Get positions for a specific strategy
    pub fn get_positions_by_strategy(&self, strategy_id: Uuid) -> Vec<OpenPosition> {
        self.open_positions.values()
            .filter(|p| p.strategy_id == strategy_id)
            .cloned()
            .collect()
    }
    
    /// Get positions for a specific instrument
    pub fn get_positions_by_instrument(&self, instrument: &str) -> Vec<OpenPosition> {
        self.open_positions.values()
            .filter(|p| p.instrument == instrument)
            .cloned()
            .collect()
    }
    
    /// Remove oldest position for a specific instrument and bot
    /// Returns the position if found and removed
    pub fn remove_position_by_instrument(&mut self, instrument: &str, bot_id: Uuid) -> Option<OpenPosition> {
        // Find the oldest position for this instrument and bot (FIFO)
        let position_to_remove = self.open_positions.values()
            .filter(|p| p.instrument == instrument && p.bot_id == bot_id)
            .min_by_key(|p| p.opened_at)
            .map(|p| p.position_id);
        
        if let Some(position_id) = position_to_remove {
            self.remove_position(position_id)
        } else {
            None
        }
    }
    
    /// Count total open positions
    pub fn position_count(&self) -> usize {
        self.open_positions.len()
    }
    
    /// Clear all positions (for testing or reset)
    pub fn clear(&mut self) {
        self.open_positions.clear();
    }
    
    /// Get exposure breakdown by asset
    pub fn exposure_breakdown(&self) -> HashMap<String, f64> {
        let mut breakdown = HashMap::new();
        
        for pos in self.open_positions.values() {
            // Extract asset name (e.g., "XAU" from "XAU_USD")
            let asset = pos.instrument.split('_').next().unwrap_or(&pos.instrument);
            *breakdown.entry(asset.to_string()).or_insert(0.0) += pos.notional_value;
        }
        
        breakdown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_position_tracker() {
        let mut tracker = PositionTracker::new();
        
        let bot_id = Uuid::new_v4();
        let strategy_id = Uuid::new_v4();
        
        // Add gold position
        let gold_pos = OpenPosition::new(
            bot_id,
            strategy_id,
            "XAU_USD",
            "LONG",
            2000.0,
            0.1,
        );
        tracker.add_position(gold_pos.clone());
        
        // Add BTC position
        let btc_pos = OpenPosition::new(
            bot_id,
            strategy_id,
            "BTC_USD",
            "SHORT",
            40000.0,
            0.5,
        );
        tracker.add_position(btc_pos.clone());
        
        // Check total exposure
        assert_eq!(tracker.position_count(), 2);
        assert!(tracker.total_exposure() > 0.0);
        
        // Check asset-specific exposure
        let gold_exposure = tracker.exposure_by_asset("XAU");
        assert_eq!(gold_exposure, 2000.0 * 0.1 * 100.0); // 100 oz per lot
        
        let btc_exposure = tracker.exposure_by_asset("BTC");
        assert_eq!(btc_exposure, 40000.0 * 0.5 * 1.0); // 1 BTC per lot
        
        // Remove position
        tracker.remove_position(gold_pos.position_id);
        assert_eq!(tracker.position_count(), 1);
    }
}
