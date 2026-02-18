//! Analytics Module - Performance tracking and adaptive learning
//!
//! Analyzes trade context data to provide insights on:
//! - Win rate by condition (RSI, ADX, regime, session, etc.)
//! - Recommendations for parameter adjustments
//! - Performance trends over time

pub mod adaptation;
pub mod learning;
pub mod performance;
pub mod ensemble;
pub mod sentiment_scorer;
pub mod correlation;
pub mod evolution;
pub mod trailing_stop;
pub mod trade_analyzer;

pub use adaptation::*;
pub use learning::*;
pub use performance::*;
pub use ensemble::*;
pub use sentiment_scorer::*;
pub use correlation::*;
pub use evolution::*;
pub use trailing_stop::*;
pub use trade_analyzer::*;
