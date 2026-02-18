//! Market Data Feeds
//! 
//! External data sources for MRATE engine enrichment

pub mod alternative_me;
pub mod calendar;
pub mod crypto;
pub mod dxy;
pub mod event_learner;
pub mod fear_greed;
pub mod fred;
pub mod gld;
pub mod reddit_sentiment;
pub mod vix;

pub use alternative_me::{AlternativeMeClient, FearGreedData as AltFearGreedData};
pub use calendar::{EconomicCalendarClient, EconomicEvent, EventImpact};
pub use crypto::CryptoClient;
pub use dxy::{DxyClient, DxyData};
pub use event_learner::{EventLearner, EventImpact as LearnedEventImpact, KnownHighImpactEvents};
pub use fear_greed::{FearGreedClient, FearGreedIndex};
pub use fred::FredClient;
pub use gld::{GldClient, GldFlowData};
pub use reddit_sentiment::{RedditSentimentClient, RedditSentiment};
pub use vix::VixClient;
