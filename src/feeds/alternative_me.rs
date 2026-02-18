use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

/// Alternative.me Crypto Fear & Greed Index
/// Free API, no key required
/// Docs: https://alternative.me/crypto/fear-and-greed-index/

const API_URL: &str = "https://api.alternative.me/fng/?limit=1";

#[derive(Debug, Clone)]
pub struct AlternativeMeClient {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FearGreedData {
    pub value: f64,
    pub value_classification: String,
    pub timestamp: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    data: Vec<RawFearGreed>,
}

#[derive(Debug, Deserialize)]
struct RawFearGreed {
    value: String,
    value_classification: String,
    timestamp: String,
}

impl AlternativeMeClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
    
    /// Fetch latest Fear & Greed Index (0-100)
    /// 0-24 = Extreme Fear
    /// 25-49 = Fear
    /// 50-74 = Greed
    /// 75-100 = Extreme Greed
    pub async fn get_fear_greed(&self) -> Option<FearGreedData> {
        let response = match self.client
            .get(API_URL)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Alternative.me request failed: {}", e);
                return None;
            }
        };
        
        if !response.status().is_success() {
            warn!("Alternative.me returned {}", response.status());
            return None;
        }
        
        let api_response: ApiResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse Alternative.me response: {}", e);
                return None;
            }
        };
        
        api_response.data.first().and_then(|raw| {
            let value = raw.value.parse::<f64>().ok()?;
            Some(FearGreedData {
                value,
                value_classification: raw.value_classification.clone(),
                timestamp: raw.timestamp.clone(),
            })
        })
    }
}

impl Default for AlternativeMeClient {
    fn default() -> Self {
        Self::new()
    }
}
