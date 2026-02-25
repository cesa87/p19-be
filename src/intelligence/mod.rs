//! Intelligence Engine — Real-time market intelligence aggregator
//!
//! Aggregates all data feeds into unified tension/opportunity scores,
//! detects anomalies, tracks whale moves, and scans prediction markets for arb.

pub mod aggregator;
pub mod scorer;
pub mod whale_tracker;
pub mod event_detector;
pub mod arb_scanner;
pub mod api_routes;

pub use aggregator::{FeedSnapshot, IntelligenceAggregator};
pub use scorer::{IntelligenceScore, IntelligenceScorer};
pub use arb_scanner::{ArbOpportunity, ArbScanner};
pub use whale_tracker::{WhaleAlert, WhaleTracker};
pub use event_detector::{IntelligenceEvent, EventDetector};
pub mod settings;
pub mod gate;

pub use settings::{get_bot_intelligence_gate, set_bot_intelligence_gate, get_gate_thresholds, GateThresholds};
pub use gate::{evaluate_gate, instrument_to_intel_key, IntelligenceGateDecision, GateAction};
