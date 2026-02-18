use grammers_client::Client;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{info, warn, error};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::signal_parser::{SignalParser, TradeSignal};
use super::copy_executor::CopyExecutor;
use crate::broker::oanda::OandaClient;

/// A signal received from a watched channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSignal {
    pub id: String,
    pub signal: TradeSignal,
    pub channel_username: String,
    pub channel_title: String,
    pub message_id: i32,
    pub message_text: String,
    pub received_at: DateTime<Utc>,
    pub executed: bool,
    pub execution_result: Option<ExecutionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub order_id: Option<String>,
    pub fill_price: Option<f64>,
    pub error: Option<String>,
    pub executed_at: DateTime<Utc>,
}

/// Channel message (signal or regular)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: i32,
    pub channel_username: String,
    pub channel_title: String,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    pub is_signal: bool,
    pub parsed_signal: Option<TradeSignal>,
}

/// Configuration for the signal listener
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenerConfig {
    pub channel_username: String,
    pub auto_execute: bool,
    pub min_confidence: f64,
    pub lot_size: f64,
    pub max_daily_trades: u32,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            channel_username: String::new(), // User must select a channel
            auto_execute: false,
            min_confidence: 0.7,
            lot_size: 0.1,
            max_daily_trades: 3,
        }
    }
}

/// Listener state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ListenerState {
    Stopped,
    Starting,
    Running,
    Error(String),
}

/// Real-time signal listener using MTProto updates
pub struct SignalListener {
    parser: SignalParser,
    config: Arc<RwLock<ListenerConfig>>,
    state: Arc<RwLock<ListenerState>>,
    // Broadcast channel for new signals
    signal_tx: broadcast::Sender<LiveSignal>,
    // Broadcast channel for all messages (for UI feed)
    message_tx: broadcast::Sender<ChannelMessage>,
    // Recent signals buffer
    recent_signals: Arc<Mutex<Vec<LiveSignal>>>,
    // Recent messages buffer (for UI)
    recent_messages: Arc<Mutex<Vec<ChannelMessage>>>,
    // Stop signal
    stop_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl SignalListener {
    pub fn new() -> Self {
        let (signal_tx, _) = broadcast::channel(100);
        let (message_tx, _) = broadcast::channel(100);
        
        Self {
            parser: SignalParser::new(),
            config: Arc::new(RwLock::new(ListenerConfig::default())),
            state: Arc::new(RwLock::new(ListenerState::Stopped)),
            signal_tx,
            message_tx,
            recent_signals: Arc::new(Mutex::new(Vec::new())),
            recent_messages: Arc::new(Mutex::new(Vec::new())),
            stop_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Get current state
    pub async fn get_state(&self) -> ListenerState {
        self.state.read().await.clone()
    }

    /// Get current config
    pub async fn get_config(&self) -> ListenerConfig {
        self.config.read().await.clone()
    }

    /// Update config
    pub async fn update_config(&self, config: ListenerConfig) {
        *self.config.write().await = config;
    }

    /// Subscribe to new signals
    pub fn subscribe_signals(&self) -> broadcast::Receiver<LiveSignal> {
        self.signal_tx.subscribe()
    }

    /// Subscribe to all messages
    pub fn subscribe_messages(&self) -> broadcast::Receiver<ChannelMessage> {
        self.message_tx.subscribe()
    }

    /// Get recent signals
    pub async fn get_recent_signals(&self, limit: usize) -> Vec<LiveSignal> {
        let signals = self.recent_signals.lock().await;
        signals.iter().rev().take(limit).cloned().collect()
    }

    /// Get recent messages
    pub async fn get_recent_messages(&self, limit: usize) -> Vec<ChannelMessage> {
        let messages = self.recent_messages.lock().await;
        messages.iter().rev().take(limit).cloned().collect()
    }

    /// Mark a signal as executed
    pub async fn mark_executed(&self, signal_id: &str, result: ExecutionResult) {
        let mut signals = self.recent_signals.lock().await;
        if let Some(signal) = signals.iter_mut().find(|s| s.id == signal_id) {
            signal.executed = true;
            signal.execution_result = Some(result);
        }
    }

    /// Start listening to a channel
    pub async fn start(&self, client: Arc<Client>, oanda: Option<Arc<OandaClient>>) -> Result<(), String> {
        // Check if already running
        {
            let state = self.state.read().await;
            if *state == ListenerState::Running {
                return Err("Listener already running".to_string());
            }
        }

        let config = self.get_config().await;
        let channel_username = config.channel_username.trim_start_matches('@').to_string();
        
        // Check if channel is configured
        if channel_username.is_empty() {
            return Err("No channel configured. Please select a channel to monitor first.".to_string());
        }

        *self.state.write().await = ListenerState::Starting;
        
        info!("Starting signal listener for @{}...", channel_username);

        // Resolve the channel
        let channel = client.resolve_username(&channel_username)
            .await
            .map_err(|e| format!("Failed to resolve channel: {}", e))?
            .ok_or_else(|| format!("Channel @{} not found", channel_username))?;

        let channel_title = match &channel {
            grammers_client::types::Chat::Channel(c) => c.title().to_string(),
            _ => channel_username.clone(),
        };

        info!("Resolved channel: {} ({})", channel_title, channel_username);

        // Create stop channel
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        *self.stop_tx.lock().await = Some(stop_tx);

        // Clone what we need for the task
        let parser = SignalParser::new();
        let signal_tx = self.signal_tx.clone();
        let message_tx = self.message_tx.clone();
        let recent_signals = self.recent_signals.clone();
        let recent_messages = self.recent_messages.clone();
        let state = self.state.clone();
        let config_arc = self.config.clone();
        let channel_username_clone = channel_username.clone();
        let channel_title_clone = channel_title.clone();
        
        // Create executor for auto-execution if OANDA client is available
        let executor = oanda.map(|broker| {
            Arc::new(CopyExecutor::new(broker, config_arc.clone()))
        });

        // Spawn the listener task
        tokio::spawn(async move {
            *state.write().await = ListenerState::Running;
            info!("Signal listener running for @{}", channel_username_clone);

            // Use grammers update loop
            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        info!("Signal listener stopping (stop signal received)");
                        break;
                    }
                    update = client.next_update() => {
                        match update {
                            Ok(Some(update)) => {
                                // Check if it's a new message in our channel
                                if let grammers_client::Update::NewMessage(message) = update {
                                    // Check if message is from our channel
                                    let chat = message.chat();
                                    let is_our_channel = match &chat {
                                        grammers_client::types::Chat::Channel(c) => {
                                            c.username().map(|u| u.eq_ignore_ascii_case(&channel_username_clone)).unwrap_or(false)
                                        }
                                        _ => false,
                                    };

                                    if is_our_channel {
                                        let text = message.text().to_string();
                                        let msg_id = message.id();
                                        
                                        info!("New message from @{}: {}", channel_username_clone, &text[..text.len().min(50)]);

                                        // Try to parse as signal
                                        let parsed = parser.parse(&text);
                                        let is_signal = parsed.is_some();
                                        let config = config_arc.read().await;

                                        // Create channel message
                                        let channel_msg = ChannelMessage {
                                            id: msg_id,
                                            channel_username: channel_username_clone.clone(),
                                            channel_title: channel_title_clone.clone(),
                                            text: text.clone(),
                                            timestamp: Utc::now(),
                                            is_signal,
                                            parsed_signal: parsed.clone(),
                                        };

                                        // Store and broadcast message
                                        {
                                            let mut messages = recent_messages.lock().await;
                                            messages.push(channel_msg.clone());
                                            // Keep last 100 messages
                                            if messages.len() > 100 {
                                                messages.remove(0);
                                            }
                                        }
                                        let _ = message_tx.send(channel_msg);

                                        // If it's a valid signal, create LiveSignal
                                        if let Some(signal) = parsed {
                                            info!("📊 Signal parsed with confidence: {:.0}% (min required: {:.0}%)", 
                                                signal.confidence * 100.0, config.min_confidence * 100.0);
                                            if signal.confidence >= config.min_confidence {
                                                let live_signal = LiveSignal {
                                                    id: format!("{}-{}", channel_username_clone, msg_id),
                                                    signal,
                                                    channel_username: channel_username_clone.clone(),
                                                    channel_title: channel_title_clone.clone(),
                                                    message_id: msg_id,
                                                    message_text: text,
                                                    received_at: Utc::now(),
                                                    executed: false,
                                                    execution_result: None,
                                                };

                                                info!("🎯 SIGNAL DETECTED: {:?} {} @ {:?}", 
                                                    live_signal.signal.direction,
                                                    live_signal.signal.symbol,
                                                    live_signal.signal.entry_price
                                                );

                                                // Store signal
                                                {
                                                    let mut signals = recent_signals.lock().await;
                                                    signals.push(live_signal.clone());
                                                    // Keep last 50 signals
                                                    if signals.len() > 50 {
                                                        signals.remove(0);
                                                    }
                                                }

                                                // Broadcast signal
                                                let _ = signal_tx.send(live_signal.clone());
                                                
                                                // Auto-execute if enabled
                                                info!("📋 Auto-execute setting: {}, OANDA executor available: {}", 
                                                    config.auto_execute, executor.is_some());
                                                
                                                if config.auto_execute {
                                                    if let Some(ref exec) = executor {
                                                        info!("⚡ Auto-executing signal: {:?} {}", 
                                                            live_signal.signal.direction, 
                                                            live_signal.signal.symbol);
                                                        let result = exec.execute(&live_signal).await;
                                                        if result.success {
                                                            info!("✅ Auto-executed: fill_price={:?}", result.fill_price);
                                                        } else {
                                                            warn!("❌ Auto-execute failed: {:?}", result.error);
                                                        }
                                                        // Mark as executed
                                                        {
                                                            let mut signals = recent_signals.lock().await;
                                                            if let Some(sig) = signals.iter_mut().find(|s| s.id == live_signal.id) {
                                                                sig.executed = true;
                                                                sig.execution_result = Some(result);
                                                            }
                                                        }
                                                    } else {
                                                        warn!("Auto-execute enabled but no OANDA client available");
                                                    }
                                                }
                                            } else {
                                                info!("⏭️ Signal skipped - confidence too low");
                                            }
                                        } else {
                                            info!("📝 Message not recognized as a trade signal");
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                // No update available, continue
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                
                                // Check for auth errors
                                if err_str.contains("AUTH_KEY_UNREGISTERED") || err_str.contains("AUTH_KEY_INVALID") {
                                    error!("⚠️ Telegram session invalidated: {}. Listener will stop - please re-authenticate via /api/telegram/auth/*", e);
                                    *state.write().await = ListenerState::Error("Session invalidated - re-auth required".to_string());
                                    break;
                                } else {
                                    error!("Error receiving update: {}", e);
                                    // Brief pause before retrying for other errors
                                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                                }
                            }
                        }
                    }
                }
            }

            *state.write().await = ListenerState::Stopped;
            info!("Signal listener stopped");
        });

        Ok(())
    }

    /// Stop the listener
    pub async fn stop(&self) -> Result<(), String> {
        let state = self.state.read().await;
        if *state != ListenerState::Running {
            return Err("Listener not running".to_string());
        }
        drop(state);

        // Send stop signal
        let stop_tx = self.stop_tx.lock().await.take();
        if let Some(tx) = stop_tx {
            let _ = tx.send(());
        }

        Ok(())
    }
}

impl Default for SignalListener {
    fn default() -> Self {
        Self::new()
    }
}
