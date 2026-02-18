pub mod polymarket;
pub mod history;

pub use polymarket::{PolymarketClient, MacroSentiment, Market};
pub use history::{MacroHistoryService, MacroRegimeState, MacroRegime, MacroSnapshot, calculate_regime_from_sentiment};
