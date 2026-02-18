//! Dynamic Threshold Engine (DTE)
//! 
//! Converts MRATE scores into strategy parameter adjustments.
//! This creates portfolio-level behavior, not just indicator behavior.

use serde::{Deserialize, Serialize};

/// Dynamic thresholds calculated from MRATE scores
/// These modulate all strategy parameters based on market regime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicThresholds {
    /// Market aggression score (0-100)
    /// 0-40 = Defensive, 40-70 = Normal, 70-100 = Aggressive
    pub market_aggression: f64,
    
    /// Entry softening (-0.4 to 0.6)
    /// Negative = harder entries (panic), Positive = easier entries (bull)
    pub entry_softening: f64,
    
    /// Breakout aggression (0.0 to 1.0)
    /// Higher = enter breakouts earlier
    pub breakout_aggression: f64,
    
    /// Mean reversion aggression (0.0 to 1.0)
    /// Higher = more aggressive mean reversion (low liquidity = ranging markets)
    pub mean_reversion_aggression: f64,
    
    /// Stop loss multiplier (0.9 to 1.2)
    /// Strong markets = tighter stops, uncertain = wider stops
    pub stop_multiplier: f64,
    
    /// Take profit multiplier (1.0 to 1.4)
    /// Strong markets = let winners run
    pub tp_multiplier: f64,
}

impl Default for DynamicThresholds {
    fn default() -> Self {
        Self {
            market_aggression: 50.0,
            entry_softening: 0.0,
            breakout_aggression: 0.5,
            mean_reversion_aggression: 0.5,
            stop_multiplier: 1.0,
            tp_multiplier: 1.0,
        }
    }
}

impl DynamicThresholds {
    /// Calculate dynamic thresholds from MRATE scores
    pub fn calculate(liquidity_score: f64, risk_score: f64, uncertainty_score: f64) -> Self {
        // ═══════════════════════════════════════════════════════════════════
        // STEP 1: Master Aggression Score
        // Combines liquidity + inverse uncertainty
        // High liquidity + low uncertainty = aggressive trending market
        // ═══════════════════════════════════════════════════════════════════
        let market_aggression = (liquidity_score * 0.6) + ((100.0 - uncertainty_score) * 0.4);
        let market_aggression = market_aggression.clamp(0.0, 100.0);
        
        // ═══════════════════════════════════════════════════════════════════
        // STEP 2: Entry Softening
        // Controls how far from ideal the signal can be
        // Panic = harder entries (-0.4), Super bull = easier entries (+0.6)
        // ═══════════════════════════════════════════════════════════════════
        let entry_softening = ((market_aggression - 50.0) / 50.0).clamp(-0.4, 0.6);
        
        // ═══════════════════════════════════════════════════════════════════
        // STEP 3: Breakout Aggression
        // Strong markets = enter breakouts earlier
        // ═══════════════════════════════════════════════════════════════════
        let breakout_aggression = (market_aggression / 100.0).clamp(0.0, 1.0);
        
        // ═══════════════════════════════════════════════════════════════════
        // STEP 4: Mean Reversion Aggression
        // Low liquidity = ranging markets = more MR opportunities
        // ═══════════════════════════════════════════════════════════════════
        let mean_reversion_aggression = ((100.0 - liquidity_score) / 100.0).clamp(0.0, 1.0);
        
        // ═══════════════════════════════════════════════════════════════════
        // STEP 5: Dynamic Stops (CRITICAL)
        // Strong markets = tighter stops (let winners run)
        // Uncertain markets = wider stops (give room to breathe)
        // Range: 0.9 - 1.2
        // ═══════════════════════════════════════════════════════════════════
        let stop_multiplier = (1.2 - (market_aggression / 100.0 * 0.3)).clamp(0.9, 1.2);
        
        // ═══════════════════════════════════════════════════════════════════
        // STEP 6: Dynamic Take Profit (CRITICAL)
        // Strong markets = extend targets (let winners run)
        // Uncertain markets = take profits earlier
        // Range: 1.0 - 1.4
        // ═══════════════════════════════════════════════════════════════════
        let tp_multiplier = (1.0 + (market_aggression / 100.0 * 0.4)).clamp(1.0, 1.4);
        
        Self {
            market_aggression,
            entry_softening,
            breakout_aggression,
            mean_reversion_aggression,
            stop_multiplier,
            tp_multiplier,
        }
    }
    
    /// Apply entry softening to RSI oversold threshold
    /// Base RSI 30 → Panic: 27, Neutral: 30, Bull: 33, Super bull: 36
    pub fn adjust_rsi_oversold(&self, base: f64) -> f64 {
        let adjusted = base + (self.entry_softening * 10.0);
        // Safety rail: never allow RSI > 40 for oversold
        adjusted.clamp(20.0, 40.0)
    }
    
    /// Apply entry softening to RSI overbought threshold
    /// Base RSI 70 → Panic: 73, Neutral: 70, Bull: 67, Super bull: 64
    pub fn adjust_rsi_overbought(&self, base: f64) -> f64 {
        let adjusted = base - (self.entry_softening * 10.0);
        // Safety rail: never allow RSI < 60 for overbought
        adjusted.clamp(60.0, 80.0)
    }
    
    /// Apply entry softening to ADX threshold
    /// Stronger markets = lower ADX requirement
    pub fn adjust_adx_threshold(&self, base: f64) -> f64 {
        let adjusted = base - (self.entry_softening * 5.0);
        // Safety rail: never below 15
        adjusted.clamp(15.0, 30.0)
    }
    
    /// Apply breakout aggression to breakout buffer
    /// Strong markets = smaller buffer (enter earlier)
    pub fn adjust_breakout_buffer(&self, base_atr_mult: f64) -> f64 {
        let adjusted = base_atr_mult * (1.2 - self.breakout_aggression * 0.5);
        adjusted.clamp(0.5, 1.5)
    }
    
    /// Apply stop multiplier to ATR-based stop loss
    pub fn adjust_stop_loss(&self, base_atr_mult: f64) -> f64 {
        let adjusted = base_atr_mult * self.stop_multiplier;
        // Safety rail: never < 0.8 ATR
        adjusted.clamp(0.8, 3.0)
    }
    
    /// Apply TP multiplier to ATR-based take profit
    pub fn adjust_take_profit(&self, base_atr_mult: f64) -> f64 {
        let adjusted = base_atr_mult * self.tp_multiplier;
        // Safety rail: never > 5 ATR (reasonable max)
        adjusted.clamp(1.0, 5.0)
    }
    
    /// Get regime description based on market aggression
    pub fn regime_description(&self) -> &'static str {
        if self.market_aggression >= 70.0 {
            "AGGRESSIVE (trending)"
        } else if self.market_aggression >= 40.0 {
            "NORMAL"
        } else {
            "DEFENSIVE (cautious)"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_panic_regime() {
        // Low liquidity, high uncertainty
        let dt = DynamicThresholds::calculate(30.0, 70.0, 80.0);
        assert!(dt.market_aggression < 40.0);
        assert!(dt.entry_softening < 0.0);
        assert!(dt.stop_multiplier > 1.0); // Wider stops in panic
        assert!(dt.tp_multiplier < 1.2); // More conservative TP
    }
    
    #[test]
    fn test_bull_regime() {
        // High liquidity, low uncertainty
        let dt = DynamicThresholds::calculate(80.0, 30.0, 30.0);
        assert!(dt.market_aggression > 60.0);
        assert!(dt.entry_softening > 0.0);
        assert!(dt.stop_multiplier < 1.1); // Tighter stops
        assert!(dt.tp_multiplier > 1.2); // Extended TP
    }
    
    #[test]
    fn test_rsi_adjustments() {
        let bull = DynamicThresholds::calculate(80.0, 30.0, 30.0);
        let panic = DynamicThresholds::calculate(30.0, 70.0, 80.0);
        
        // Bull should have higher RSI oversold (easier entry)
        assert!(bull.adjust_rsi_oversold(30.0) > panic.adjust_rsi_oversold(30.0));
        
        // Bull should have lower RSI overbought (easier short entry)  
        assert!(bull.adjust_rsi_overbought(70.0) < panic.adjust_rsi_overbought(70.0));
    }
}
