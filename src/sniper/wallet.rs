//! Wallet session management
//! 
//! Handles wallet connections from Reown (frontend) and maintains session state.
//! The actual signing happens on the frontend - backend just tracks sessions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Connected wallet session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSession {
    pub session_id: String,
    pub pubkey: String,
    pub connected_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub network: String,  // "mainnet" or "devnet"
}

/// In-memory wallet session store
pub struct WalletSessionStore {
    sessions: RwLock<HashMap<String, WalletSession>>,
}

impl WalletSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new wallet session
    pub async fn create_session(&self, pubkey: String, network: String) -> WalletSession {
        let session = WalletSession {
            session_id: Uuid::new_v4().to_string(),
            pubkey,
            connected_at: Utc::now(),
            last_active: Utc::now(),
            network,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_id.clone(), session.clone());
        session
    }

    /// Get session by ID
    pub async fn get_session(&self, session_id: &str) -> Option<WalletSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Get session by pubkey
    pub async fn get_session_by_pubkey(&self, pubkey: &str) -> Option<WalletSession> {
        let sessions = self.sessions.read().await;
        sessions.values()
            .find(|s| s.pubkey == pubkey)
            .cloned()
    }

    /// Update last active timestamp
    pub async fn touch_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_active = Utc::now();
        }
    }

    /// Remove session
    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<WalletSession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }
}

impl Default for WalletSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Create shared wallet session store
pub fn create_wallet_store() -> Arc<WalletSessionStore> {
    Arc::new(WalletSessionStore::new())
}
