//! Risk management data models
//!
//! Core data structures for the Portfolio Risk Engine

use chrono::{DateTime, Utc, Datelike};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Account state for risk monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    /// Current account equity (balance + unrealized P&L)
    pub equity: f64,
    /// Account balance (realized only)
    pub balance: f64,
    /// Peak equity seen (for drawdown calculation)
    pub peak_equity: f64,
    /// Profit/loss for current day
    pub daily_pnl: f64,
    /// Profit/loss for current week
    pub weekly_pnl: f64,
    /// Number of consecutive losing trades
    pub consecutive_losses: u32,
    /// Total trades executed
    pub total_trades: u32,
    /// Number of winning trades
    pub winning_trades: u32,
    /// Last time daily stats were reset (UTC midnight)
    pub last_reset_daily: DateTime<Utc>,
    /// Last time weekly stats were reset (UTC Sunday midnight)
    pub last_reset_weekly: DateTime<Utc>,
    /// Emergency kill switch status
    pub kill_switch_active: bool,
    /// Last trade outcome (for consecutive loss tracking)
    pub last_trade_pnl: Option<f64>,
}

impl Default for AccountState {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            equity: 10000.0,  // Default starting equity
            balance: 10000.0,
            peak_equity: 10000.0,
            daily_pnl: 0.0,
            weekly_pnl: 0.0,
            consecutive_losses: 0,
            total_trades: 0,
            winning_trades: 0,
            last_reset_daily: now,
            last_reset_weekly: now,
            kill_switch_active: false,
            last_trade_pnl: None,
        }
    }
}

impl AccountState {
    pub fn new(starting_equity: f64) -> Self {
        let now = Utc::now();
        Self {
            equity: starting_equity,
            balance: starting_equity,
            peak_equity: starting_equity,
            daily_pnl: 0.0,
            weekly_pnl: 0.0,
            consecutive_losses: 0,
            total_trades: 0,
            winning_trades: 0,
            last_reset_daily: now,
            last_reset_weekly: now,
            kill_switch_active: false,
            last_trade_pnl: None,
        }
    }
    
    /// Calculate current drawdown as percentage from peak
    pub fn current_drawdown_pct(&self) -> f64 {
        if self.peak_equity == 0.0 {
            return 0.0;
        }
        ((self.peak_equity - self.equity) / self.peak_equity * 100.0).max(0.0)
    }
    
    /// Calculate daily loss as percentage of starting equity
    pub fn daily_loss_pct(&self) -> f64 {
        if self.peak_equity == 0.0 {
            return 0.0;
        }
        // Daily loss as % of peak (not current equity)
        (self.daily_pnl / self.peak_equity * 100.0).min(0.0).abs()
    }
    
    /// Calculate weekly loss as percentage
    pub fn weekly_loss_pct(&self) -> f64 {
        if self.peak_equity == 0.0 {
            return 0.0;
        }
        (self.weekly_pnl / self.peak_equity * 100.0).min(0.0).abs()
    }
    
    /// Update equity and check if peak needs updating
    pub fn update_equity(&mut self, new_equity: f64) {
        self.equity = new_equity;
        if new_equity > self.peak_equity {
            self.peak_equity = new_equity;
        }
    }
    
    /// Record a completed trade
    pub fn record_trade(&mut self, pnl: f64) {
        self.daily_pnl += pnl;
        self.weekly_pnl += pnl;
        self.balance += pnl;
        self.update_equity(self.balance);
        
        // Track total trades and wins
        self.total_trades += 1;
        if pnl > 0.0 {
            self.winning_trades += 1;
            self.consecutive_losses = 0;
        } else if pnl < 0.0 {
            self.consecutive_losses += 1;
        }
        
        self.last_trade_pnl = Some(pnl);
    }
    
    /// Reset daily statistics (called at UTC midnight)
    pub fn reset_daily(&mut self) {
        self.daily_pnl = 0.0;
        self.last_reset_daily = Utc::now();
    }
    
    /// Reset weekly statistics (called at UTC Sunday midnight)
    pub fn reset_weekly(&mut self) {
        self.weekly_pnl = 0.0;
        self.last_reset_weekly = Utc::now();
    }
    
    /// Check if daily reset is needed
    pub fn needs_daily_reset(&self) -> bool {
        let now = Utc::now();
        now.date_naive() > self.last_reset_daily.date_naive()
    }
    
    /// Check if weekly reset is needed (Sunday midnight UTC)
    pub fn needs_weekly_reset(&self) -> bool {
        let now = Utc::now();
        let days_since = (now - self.last_reset_weekly).num_days();
        days_since >= 7 || (days_since > 0 && now.weekday() == chrono::Weekday::Sun)
    }
}

/// Risk decision made by the engine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskDecision {
    /// Trading allowed at full size
    AllowTrading,
    /// Trading allowed but reduce position size by this multiplier (0.0-1.0)
    ReducePositionSize(f64),
    /// Block all new trades but keep existing positions
    BlockNewTrades,
    /// Emergency shutdown - close all positions immediately
    EmergencyShutdown,
}

/// Risk limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    /// Maximum daily loss as % of equity (e.g., 3.0 = -3%)
    pub max_daily_loss_pct: f64,
    /// Maximum weekly loss as % of equity (e.g., 6.0 = -6%)
    pub max_weekly_loss_pct: f64,
    /// Maximum drawdown as % from peak (e.g., 10.0 = -10%)
    pub max_drawdown_pct: f64,
    /// Maximum consecutive losing trades
    pub max_consecutive_losses: u32,
    /// Maximum total exposure as multiplier of equity (e.g., 5.0 = 5x)
    pub max_total_exposure_multiplier: f64,
    /// Maximum exposure per asset as multiplier of equity (e.g., 2.0 = 2x)
    pub max_per_asset_multiplier: f64,
    /// Maximum exposure per strategy as multiplier of equity (e.g., 1.5 = 1.5x)
    pub max_per_strategy_multiplier: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_daily_loss_pct: 3.0,
            max_weekly_loss_pct: 6.0,
            max_drawdown_pct: 10.0,
            max_consecutive_losses: 8,
            max_total_exposure_multiplier: 6.0,  // Increased from 5.0
            max_per_asset_multiplier: 4.0,       // Increased from 3.0
            max_per_strategy_multiplier: 3.0,
        }
    }
}

/// Current risk status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskStatus {
    pub account_state: AccountState,
    pub current_decision: RiskDecision,
    pub drawdown_pct: f64,
    pub daily_loss_pct: f64,
    pub weekly_loss_pct: f64,
    pub total_exposure: f64,
    pub exposure_pct_of_equity: f64,
    pub warnings: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

/// Open position tracked by risk engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPosition {
    pub position_id: Uuid,
    pub bot_id: Uuid,
    pub strategy_id: Uuid,
    pub instrument: String,
    pub direction: String,  // "LONG" or "SHORT"
    pub entry_price: f64,
    pub lot_size: f64,
    pub notional_value: f64,
    pub opened_at: DateTime<Utc>,
}

impl OpenPosition {
    pub fn new(
        bot_id: Uuid,
        strategy_id: Uuid,
        instrument: &str,
        direction: &str,
        entry_price: f64,
        lot_size: f64,
    ) -> Self {
        // Calculate notional value
        // For gold: 1 lot = 100 oz, notional = lot_size * 100 * price
        // For BTC: 1 lot = 1 BTC, notional = lot_size * price
        let multiplier = match instrument {
            "XAU_USD" => 100.0,  // 1 gold lot = 100 oz
            "BTC_USD" => 1.0,    // 1 BTC lot = 1 BTC
            _ => 100.0,  // Default
        };
        
        let notional_value = lot_size * multiplier * entry_price;
        
        Self {
            position_id: Uuid::new_v4(),
            bot_id,
            strategy_id,
            instrument: instrument.to_string(),
            direction: direction.to_string(),
            entry_price,
            lot_size,
            notional_value,
            opened_at: Utc::now(),
        }
    }
}
