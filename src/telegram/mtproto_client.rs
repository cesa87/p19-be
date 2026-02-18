use grammers_client::{Client, Config, InitParams, SignInError};
use grammers_client::types::LoginToken;
use grammers_session::Session;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn, error};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use super::signal_parser::SignalParser;
use super::channel_monitor::HistoricalSignal;

/// Represents a Telegram channel or group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannel {
    pub id: i64,
    pub title: String,
    pub username: Option<String>,
    pub chat_type: String,  // "channel" or "group"
}

fn get_session_path() -> PathBuf {
    // Use ./data directory (same as where database is stored)
    let data_dir = Path::new("./data");
    if !data_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            error!("Failed to create data directory: {}", e);
        }
    }
    data_dir.join("telegram_session.session")
}

/// MTProto client for full Telegram API access
pub struct MtprotoClient {
    api_id: i32,
    api_hash: String,
    login_token: Option<LoginToken>,
    client: Option<Client>,
    parser: SignalParser,
}

/// Authentication state
#[derive(Debug, Clone)]
pub enum AuthState {
    NeedPhone,
    NeedCode { phone: String },
    NeedPassword { phone: String },
    Authorized,
}

impl MtprotoClient {
    pub fn new(api_id: i32, api_hash: String) -> Self {
        Self {
            api_id,
            api_hash,
            login_token: None,
            client: None,
            parser: SignalParser::new(),
        }
    }

    /// Connect to Telegram (loads existing session if available)
    pub async fn connect(&mut self) -> Result<AuthState, String> {
        info!("Connecting to Telegram MTProto...");
        
        let session_path = get_session_path();
        info!("Using session file: {}", session_path.display());
        
        // Load or create session
        let session = if session_path.exists() {
            info!("Loading existing Telegram session...");
            Session::load_file(&session_path).map_err(|e| {
                error!("Failed to load session file: {}", e);
                e.to_string()
            })?
        } else {
            info!("Creating new Telegram session...");
            Session::new()
        };

        // Create client with timeout to prevent infinite retry loops
        let connect_future = Client::connect(Config {
            session,
            api_id: self.api_id,
            api_hash: self.api_hash.clone(),
            params: InitParams {
                app_version: "1.0.0".to_string(),
                device_model: "AureumBot".to_string(),
                system_version: "1.0".to_string(),
                ..Default::default()
            },
        });
        
        // 30 second timeout for connection
        let client = match timeout(Duration::from_secs(30), connect_future).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                error!("Telegram connection failed: {}", e);
                return Err(format!("Connection failed: {}. Check your API credentials.", e));
            }
            Err(_) => {
                error!("Telegram connection timed out after 30 seconds");
                return Err("Connection timed out. Check your TELEGRAM_API_ID and TELEGRAM_API_HASH.".to_string());
            }
        };

        // Check if already authorized (with timeout)
        let authorized = match timeout(Duration::from_secs(10), client.is_authorized()).await {
            Ok(Ok(auth)) => auth,
            Ok(Err(e)) => {
                warn!("Failed to check authorization: {}", e);
                false
            }
            Err(_) => {
                warn!("Authorization check timed out");
                false
            }
        };
        
        self.client = Some(client);

        if authorized {
            info!("Already authorized with Telegram");
            Ok(AuthState::Authorized)
        } else {
            info!("Not authorized, need phone number");
            Ok(AuthState::NeedPhone)
        }
    }

    /// Start login with phone number
    pub async fn request_login_code(&mut self, phone: &str) -> Result<AuthState, String> {
        let client = self.client.as_ref().ok_or("Not connected")?;
        
        info!("Requesting login code for phone: {}", phone);
        
        let token = client.request_login_code(phone)
            .await
            .map_err(|e| {
                warn!("Failed to request login code: {}", e);
                e.to_string()
            })?;
        
        self.login_token = Some(token);
        info!("Login code requested successfully, token stored");
        
        Ok(AuthState::NeedCode { phone: phone.to_string() })
    }

    /// Complete login with verification code
    pub async fn sign_in(&mut self, code: &str) -> Result<AuthState, String> {
        info!("sign_in called, login_token present: {}", self.login_token.is_some());
        
        let client = self.client.as_ref().ok_or("Not connected")?;
        let token = self.login_token.take().ok_or_else(|| {
            warn!("Login token is None! Was request_login_code called?");
            "Login token not set. Request code first.".to_string()
        })?;
        
        info!("Attempting sign in with code: {}", code);
        
        match client.sign_in(&token, code).await {
            Ok(user) => {
                info!("Sign in successful for user: {:?}", user.first_name());
                // Save session
                let session = client.session();
                let session_path = get_session_path();
                if let Err(e) = session.save_to_file(&session_path) {
                    warn!("Failed to save session to {}: {}", session_path.display(), e);
                } else {
                    info!("Session saved to {}", session_path.display());
                }
                Ok(AuthState::Authorized)
            }
            Err(SignInError::PasswordRequired(_password_token)) => {
                warn!("2FA password required");
                Ok(AuthState::NeedPassword { phone: "unknown".to_string() })
            }
            Err(SignInError::InvalidCode) => {
                Err("Invalid verification code. Please try again.".to_string())
            }
            Err(e) => Err(format!("Sign in failed: {:?}", e)),
        }
    }

    /// List available channels and groups the user has access to
    pub async fn list_channels(&self) -> Result<Vec<TelegramChannel>, String> {
        let client = self.client.as_ref().ok_or("Not connected")?;
        
        info!("Fetching available Telegram channels/groups...");
        
        let mut channels = Vec::new();
        let mut dialogs = client.iter_dialogs();
        
        while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
            let chat = dialog.chat();
            
            // Match on the Chat enum to get channel/group info
            match chat {
                grammers_client::types::Chat::Channel(channel) => {
                    let username = channel.username().map(|u| u.to_string());
                    let title = channel.title().to_string();
                    let id = channel.id();
                    
                    channels.push(TelegramChannel {
                        id,
                        title,
                        username,
                        chat_type: "channel".to_string(),
                    });
                }
                grammers_client::types::Chat::Group(group) => {
                    let title = group.title().to_string();
                    let id = group.id();
                    
                    channels.push(TelegramChannel {
                        id,
                        title,
                        username: None,  // Groups don't have usernames
                        chat_type: "group".to_string(),
                    });
                }
                _ => {
                    // Skip private chats
                }
            }
        }
        
        info!("Found {} channels/groups", channels.len());
        Ok(channels)
    }

    /// Get messages from a public channel
    pub async fn get_channel_messages(
        &self,
        channel_username: &str,
        limit: usize,
    ) -> Result<Vec<HistoricalSignal>, String> {
        let client = self.client.as_ref().ok_or("Not connected")?;
        
        // Resolve the channel
        let channel = client.resolve_username(channel_username)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Channel @{} not found", channel_username))?;
        
        info!("Fetching messages from @{}...", channel_username);
        
        let mut signals = Vec::new();
        let mut messages = client.iter_messages(&channel);
        let mut count = 0;
        
        while let Some(message) = messages.next().await.map_err(|e| e.to_string())? {
            if count >= limit {
                break;
            }
            
            let text = message.text();
            if !text.is_empty() {
                if let Some(signal) = self.parser.parse(text) {
                    signals.push(HistoricalSignal {
                        signal,
                        timestamp: message.date().timestamp(),
                        message_id: message.id() as i64,
                        channel_username: channel_username.to_string(),
                    });
                }
            }
            count += 1;
        }
        
        info!("Found {} signals in {} messages from @{}", signals.len(), count, channel_username);
        Ok(signals)
    }

    /// Check if client is authorized (with timeout to prevent hangs)
    pub async fn is_authorized(&self) -> bool {
        if let Some(ref client) = self.client {
            match timeout(Duration::from_secs(5), client.is_authorized()).await {
                Ok(Ok(auth)) => auth,
                Ok(Err(_)) => false,
                Err(_) => {
                    warn!("is_authorized check timed out");
                    false
                }
            }
        } else {
            false
        }
    }
    
    /// Get the underlying grammers client (for signal listener)
    pub fn get_client(&self) -> Option<&Client> {
        self.client.as_ref()
    }
    
    /// Take ownership of the grammers client
    pub fn take_client(&mut self) -> Option<Client> {
        self.client.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = MtprotoClient::new(12345, "test_hash".to_string());
        assert!(client.client.is_none());
    }
}
