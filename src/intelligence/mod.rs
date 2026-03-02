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
pub mod feed;
pub use feed::{FeedIngester, FeedSource, FeedPost, FeedPostResponse};
pub mod flights;
pub use flights::{FlightTracker, TrackedFlight};
pub mod markets;
pub use markets::{MarketsFetcher, MarketCard, MarketOutcome};
pub mod market_movers;
pub use market_movers::{MarketSnapshotTracker, MarketMover};

pub mod signal_intelligence;
pub use signal_intelligence::{SignalEngine, SignalMatch, SignalPost};
pub mod signal_tracker;
pub use signal_tracker::{SignalTracker, PersistedSignal};
pub mod rss_ingest;
pub use rss_ingest::{ingest_rss_sources, ingest_cryptopanic};
pub mod polymarket_whales;
pub use polymarket_whales::{WhaleFetcher, WhaleTrade, TrackedWallet};
