//! Bot trading engine - the core of the platform
//! 
//! This module handles:
//! - Running trading bots in background tasks
//! - Monitoring market prices
//! - Generating entry/exit signals based on strategy
//! - Placing and managing orders through OANDA

pub mod runner;
pub mod signals;
pub mod activity;

pub use runner::BotRunner;
pub use activity::BotActivity;
