//! Portfolio Risk Engine (PRE)
//!
//! Core risk management system that coordinates all trading decisions
//! at the account level to prevent catastrophic losses.
//!
//! ## Architecture
//!
//! - **PortfolioRiskEngine**: The main coordinator and kill switch
//! - **AccountState**: Tracks equity, drawdown, P&L, consecutive losses
//! - **PositionTracker**: Monitors all open positions for exposure limits
//! - **PositionSizer**: Calculates risk-based position sizing
//! - **CorrelationEngine**: Prevents correlated strategy overlap (Phase 4)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use risk::{PortfolioRiskEngine, RiskDecision};
//!
//! let risk_engine = PortfolioRiskEngine::new(10000.0);
//!
//! // Before every trade
//! match risk_engine.check_account_risk().await {
//!     RiskDecision::AllowTrading => { /* proceed */ },
//!     RiskDecision::EmergencyShutdown => { /* stop all bots */ },
//!     _ => { /* adjust position size */ },
//! }
//! ```

pub mod models;
pub mod engine;
pub mod limits;
pub mod calculator;
pub mod tracker;
pub mod correlation;

// Re-export key types for convenience
pub use models::{
    AccountState,
    RiskDecision,
    RiskLimits,
    RiskStatus,
    OpenPosition,
};

pub use engine::PortfolioRiskEngine;
pub use calculator::{PositionSizer, calculate_atr_stop, calculate_atr_take_profit};
pub use tracker::PositionTracker;
pub use correlation::CorrelationEngine;
pub use limits::{conservative_limits, aggressive_limits};
