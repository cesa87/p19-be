//! Risk calculation utilities
//!
//! Position sizing, stop loss, and take profit calculations

use crate::models::Candle;
use crate::engine::indicators::atr;

/// Position sizer using risk-based calculations
pub struct PositionSizer {
    pub account_equity: f64,
    pub risk_per_trade_pct: f64,  // e.g., 0.5%
}

impl PositionSizer {
    pub fn new(account_equity: f64, risk_per_trade_pct: f64) -> Self {
        Self {
            account_equity,
            risk_per_trade_pct,
        }
    }
    
    /// Calculate position size based on risk amount and stop distance
    /// 
    /// Formula: position_size = (account_equity * risk_pct) / stop_distance
    pub fn calculate_position_size(
        &self,
        entry_price: f64,
        stop_loss_price: f64,
        instrument: &str,
    ) -> f64 {
        let risk_amount = self.account_equity * (self.risk_per_trade_pct / 100.0);
        let stop_distance = (entry_price - stop_loss_price).abs();
        
        if stop_distance == 0.0 {
            tracing::warn!("Zero stop distance, returning minimum position size");
            return 0.01;
        }
        
        // Contract/pip value varies by instrument
        let contract_value = match instrument {
            "XAU_USD" => 100.0,  // 100 oz per lot
            "BTC_USD" => 1.0,    // 1 BTC per lot
            _ => 100.0,          // Default
        };
        
        let position_size = risk_amount / (stop_distance * contract_value);
        
        // Clamp to reasonable range
        position_size.max(0.01).min(10.0)
    }
}

/// Calculate ATR-based stop loss
/// 
/// Uses recent volatility (ATR) to set dynamic stops
pub fn calculate_atr_stop(
    candles: &[Candle],
    atr_multiplier: f64,  // e.g., 1.5 for 1.5x ATR
    direction: &str,
) -> f64 {
    let atr_values = atr(candles, 14);
    let current_atr = atr_values.last().copied().unwrap_or(0.0);
    let current_price = candles.last().map(|c| c.close).unwrap_or(0.0);
    
    if current_atr == 0.0 || current_price == 0.0 {
        return current_price;
    }
    
    match direction {
        "LONG" => current_price - (current_atr * atr_multiplier),
        "SHORT" => current_price + (current_atr * atr_multiplier),
        _ => current_price,
    }
}

/// Calculate ATR-based take profit
pub fn calculate_atr_take_profit(
    candles: &[Candle],
    atr_multiplier: f64,  // e.g., 2.0 for 2x ATR
    direction: &str,
) -> f64 {
    let atr_values = atr(candles, 14);
    let current_atr = atr_values.last().copied().unwrap_or(0.0);
    let current_price = candles.last().map(|c| c.close).unwrap_or(0.0);
    
    if current_atr == 0.0 || current_price == 0.0 {
        return current_price;
    }
    
    match direction {
        "LONG" => current_price + (current_atr * atr_multiplier),
        "SHORT" => current_price - (current_atr * atr_multiplier),
        _ => current_price,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_position_sizer() {
        let sizer = PositionSizer::new(10000.0, 0.5);
        
        // Risk $50 (0.5% of $10k) with $10 stop = 5 units... adjusted for contract size
        let pos_size = sizer.calculate_position_size(
            2000.0,  // Entry
            1990.0,  // SL (10 point stop)
            "XAU_USD",
        );
        
        // $50 risk / ($10 stop * 100 oz) = 0.05 lots
        assert!(pos_size > 0.0);
        assert!(pos_size < 1.0);
    }
}
