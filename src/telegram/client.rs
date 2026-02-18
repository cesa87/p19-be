use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use super::signal_parser::{SignalParser, TradeSignal};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";

/// Telegram Bot for receiving trade signals
pub struct TelegramBot {
    token: String,
    client: Client,
    parser: SignalParser,
    last_update_id: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Update {
    update_id: i64,
    message: Option<Message>,
    channel_post: Option<Message>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub date: i64,
    pub chat: Chat,
    pub from: Option<User>,
    pub text: Option<String>,
    pub forward_from_chat: Option<Chat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: i64,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

/// Signal received from Telegram with metadata
#[derive(Debug, Clone, Serialize)]
pub struct ReceivedSignal {
    pub signal: TradeSignal,
    pub source_chat: String,
    pub source_user: Option<String>,
    pub timestamp: i64,
    pub message_id: i64,
}

impl TelegramBot {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
            parser: SignalParser::new(),
            last_update_id: 0,
        }
    }

    /// Get bot info to verify token is valid
    pub async fn get_me(&self) -> Result<User, String> {
        let url = format!("{}{}/getMe", TELEGRAM_API_BASE, self.token);
        
        let response: TelegramResponse<User> = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        response.result.ok_or_else(|| {
            response.description.unwrap_or_else(|| "Unknown error".to_string())
        })
    }

    /// Poll for new messages (long polling)
    pub async fn get_updates(&mut self, timeout: u32) -> Result<Vec<Update>, String> {
        let url = format!(
            "{}{}/getUpdates?offset={}&timeout={}&allowed_updates=[\"message\",\"channel_post\"]",
            TELEGRAM_API_BASE, self.token, self.last_update_id + 1, timeout
        );

        let response: TelegramResponse<Vec<Update>> = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        if let Some(updates) = response.result {
            // Update the offset
            if let Some(last) = updates.last() {
                self.last_update_id = last.update_id;
            }
            Ok(updates)
        } else {
            Err(response.description.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// Send a message to a chat
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<Message, String> {
        let url = format!("{}{}/sendMessage", TELEGRAM_API_BASE, self.token);
        
        let response: TelegramResponse<Message> = self.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "HTML"
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        response.result.ok_or_else(|| {
            response.description.unwrap_or_else(|| "Unknown error".to_string())
        })
    }

    /// Process a message and extract trade signal if present
    pub fn process_message(&self, message: &Message) -> Option<ReceivedSignal> {
        let text = message.text.as_ref()?;
        
        // Try to parse the message
        let signal = self.parser.parse(text)?;
        
        // Get source info
        let source_chat = message.chat.title.clone()
            .or(message.chat.username.clone())
            .unwrap_or_else(|| message.chat.id.to_string());
        
        let source_user = message.from.as_ref().map(|u| {
            u.username.clone().unwrap_or_else(|| u.first_name.clone())
        });

        Some(ReceivedSignal {
            signal,
            source_chat,
            source_user,
            timestamp: message.date,
            message_id: message.message_id,
        })
    }

    /// Start listening for signals, sending them to a channel
    pub async fn start_listening(
        mut self,
        signal_tx: mpsc::Sender<ReceivedSignal>,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        info!("Starting Telegram bot listener...");
        
        // Verify bot is working
        match self.get_me().await {
            Ok(bot) => {
                info!("Bot connected: @{}", bot.username.unwrap_or_else(|| "unknown".to_string()));
            }
            Err(e) => {
                error!("Failed to connect bot: {}", e);
                return;
            }
        }

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Telegram bot shutting down...");
                    break;
                }
                updates = self.get_updates(30) => {
                    match updates {
                        Ok(updates) => {
                            for update in updates {
                                // Handle both direct messages and channel posts
                                let message = update.message.or(update.channel_post);
                                
                                if let Some(msg) = message {
                                    if let Some(received_signal) = self.process_message(&msg) {
                                        info!(
                                            "Signal detected: {} {} (confidence: {:.0}%)",
                                            received_signal.signal.direction,
                                            received_signal.signal.symbol,
                                            received_signal.signal.confidence * 100.0
                                        );
                                        
                                        if let Err(e) = signal_tx.send(received_signal).await {
                                            error!("Failed to send signal: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error getting updates: {}", e);
                            // Wait a bit before retrying
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_message() {
        let bot = TelegramBot::new("test_token".to_string());
        
        let message = Message {
            message_id: 1,
            date: 1234567890,
            chat: Chat {
                id: 123,
                chat_type: "group".to_string(),
                title: Some("Gold Signals".to_string()),
                username: None,
            },
            from: Some(User {
                id: 456,
                first_name: "Trader".to_string(),
                last_name: None,
                username: Some("goldtrader".to_string()),
            }),
            text: Some("BUY XAUUSD @ 2650 SL 2640 TP 2670".to_string()),
            forward_from_chat: None,
        };

        let signal = bot.process_message(&message).unwrap();
        assert_eq!(signal.source_chat, "Gold Signals");
        assert_eq!(signal.source_user, Some("goldtrader".to_string()));
    }
}
