pub mod client;
pub mod signal_parser;
pub mod channel_monitor;
pub mod mtproto_client;
pub mod strategy_discovery;
pub mod signal_listener;
pub mod copy_executor;

pub use client::TelegramBot;
pub use signal_parser::{TradeSignal, SignalParser, TradeDirection};
pub use channel_monitor::{ChannelMonitor, HistoricalSignal, SignalAnalysis};
pub use mtproto_client::{MtprotoClient, AuthState, TelegramChannel};
pub use strategy_discovery::{StrategyDiscovery, DiscoveredStrategy};
pub use signal_listener::{SignalListener, LiveSignal, ChannelMessage, ListenerConfig, ListenerState, ExecutionResult};
pub use copy_executor::CopyExecutor;
