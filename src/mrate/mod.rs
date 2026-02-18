//! MRATE - Macro-Regime Adaptive Trading Engine
//! 
//! A scoring engine that converts macro, sentiment, and volatility data into:
//! - Market Regime (GOLD_SUPER_BULL, BTC_SUPER_BULL, PANIC, TREND, CHOPPY)
//! - Strategy Allow/Block weights
//! - Dynamic Risk Multiplier

pub mod models;
pub mod engine;
pub mod scheduler;
pub mod thresholds;
pub mod orchestrator;
pub mod predictor;
pub mod activity_log;
pub mod advisor;
pub mod locked_direction;

pub use models::{
    MrateOutput, MrateInputs, Regime, StrategyCategory, StrategyWeights, TrendDirection, 
    FavoredInstrument, DataHealth, RegimeState, InstrumentScores, InstrumentScore, InstrumentRecommendation,
    PriceRegime, Timeframe, TimeframeTechnicals, TimeframeAlignment, MultiTimeframeData,
};
pub use engine::MrateEngine;
pub use scheduler::{MrateScheduler, MrateState, SharedMrateEngine, create_mrate_state, create_mrate_engine, get_current_mrate, default_mrate_output};
pub use thresholds::DynamicThresholds;
pub use predictor::{RegimePredictor, RegimePrediction};
pub use advisor::{MrateAdvisor, AdvisorOutput, Contradiction, Suggestion, BlockedOpportunity, PositionInfo};
