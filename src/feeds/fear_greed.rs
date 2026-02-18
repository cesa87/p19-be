//! Fear & Greed Index Client
//!
//! Fetches crypto Fear & Greed Index from alternative.me API
//! Also useful as a general market sentiment indicator
//! 0 = Extreme Fear, 100 = Extreme Greed

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached Fear & Greed data
struct CachedFG {
    data: Option<FearGreedIndex>,
    last_fetch: Instant,
}

// Global cache for Fear & Greed
lazy_static::lazy_static! {
    static ref FG_CACHE: Mutex<CachedFG> = Mutex::new(CachedFG {
        data: None,
        last_fetch: Instant::now() - Duration::from_secs(3600),
    });
}

const FG_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minute cache

/// Fear & Greed Index client
pub struct FearGreedClient {
    client: Client,
}

#[derive(Debug, Deserialize)]
struct FearGreedResponse {
    data: Vec<FearGreedData>,
}

#[derive(Debug, Deserialize)]
struct FearGreedData {
    value: String,
    value_classification: String,
    timestamp: String,
}

/// Fear & Greed data result
#[derive(Debug, Clone)]
pub struct FearGreedIndex {
    /// Fear & Greed value (0-100)
    pub value: f64,
    /// Classification: "Extreme Fear", "Fear", "Neutral", "Greed", "Extreme Greed"
    pub classification: String,
    /// Unix timestamp
    pub timestamp: i64,
}

impl FearGreedClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch current Fear & Greed Index (with caching)
    pub async fn get_fear_greed(&self) -> Option<FearGreedIndex> {
        // Check cache first
        {
            let cache = FG_CACHE.lock().ok()?;
            if cache.last_fetch.elapsed() < FG_CACHE_TTL {
                if cache.data.is_some() {
                    info!("Fear & Greed: Using cached value");
                    return cache.data.clone();
                }
            }
        }
        
        let url = "https://api.alternative.me/fng/?limit=1";

        match self.client.get(url).send().await {
            Ok(response) => {
                if let Ok(data) = response.json::<FearGreedResponse>().await {
                    if let Some(latest) = data.data.first() {
                        let value = latest.value.parse::<f64>().ok()?;
                        let timestamp = latest.timestamp.parse::<i64>().unwrap_or(0);

                        info!(
                            "Fear & Greed Index fetched: {} ({})",
                            value, latest.value_classification
                        );

                        let result = FearGreedIndex {
                            value,
                            classification: latest.value_classification.clone(),
                            timestamp,
                        };
                        
                        // Update cache
                        if let Ok(mut cache) = FG_CACHE.lock() {
                            cache.data = Some(result.clone());
                            cache.last_fetch = Instant::now();
                        }
                        
                        return Some(result);
                    }
                }
                warn!("Failed to parse Fear & Greed response");
                None
            }
            Err(e) => {
                warn!("Failed to fetch Fear & Greed Index: {}", e);
                // Return cached value on error
                if let Ok(cache) = FG_CACHE.lock() {
                    if cache.data.is_some() {
                        info!("Fear & Greed: Using stale cache due to error");
                        return cache.data.clone();
                    }
                }
                None
            }
        }
    }

    /// Fetch historical Fear & Greed data (up to 30 days)
    pub async fn get_fear_greed_history(&self, days: u32) -> Option<Vec<FearGreedIndex>> {
        let url = format!(
            "https://api.alternative.me/fng/?limit={}",
            days.min(30)
        );

        match self.client.get(&url).send().await {
            Ok(response) => {
                if let Ok(data) = response.json::<FearGreedResponse>().await {
                    let history: Vec<FearGreedIndex> = data
                        .data
                        .iter()
                        .filter_map(|d| {
                            let value = d.value.parse::<f64>().ok()?;
                            let timestamp = d.timestamp.parse::<i64>().unwrap_or(0);
                            Some(FearGreedIndex {
                                value,
                                classification: d.value_classification.clone(),
                                timestamp,
                            })
                        })
                        .collect();

                    if !history.is_empty() {
                        return Some(history);
                    }
                }
                None
            }
            Err(e) => {
                warn!("Failed to fetch Fear & Greed history: {}", e);
                None
            }
        }
    }

    /// Convert Fear & Greed to gold sentiment
    /// Extreme Fear = bullish gold (safe haven demand)
    /// Extreme Greed = bearish gold (risk-on environment)
    pub fn fear_greed_to_gold_score(fg_value: f64) -> f64 {
        // Inverse relationship: Fear = Gold bullish, Greed = Gold bearish
        // FG 0 (extreme fear) = Gold 90, FG 50 (neutral) = Gold 50, FG 100 (extreme greed) = Gold 10
        (90.0 - fg_value * 0.8).clamp(10.0, 90.0)
    }

    /// Convert Fear & Greed to BTC sentiment
    /// Fear = potential buying opportunity, Greed = caution
    pub fn fear_greed_to_btc_score(fg_value: f64) -> f64 {
        // Contrarian approach: buy fear, sell greed
        // But also: momentum matters. Slight adjustment to not be purely contrarian.
        // FG 20 = BTC 70 (buy the fear), FG 80 = BTC 30 (be cautious)
        (70.0 - (fg_value - 50.0) * 0.8).clamp(20.0, 80.0)
    }

    /// Determine if we're in extreme conditions
    pub fn is_extreme(fg_value: f64) -> bool {
        fg_value < 25.0 || fg_value > 75.0
    }

    /// Get sentiment category
    pub fn get_category(fg_value: f64) -> &'static str {
        if fg_value < 20.0 {
            "EXTREME_FEAR"
        } else if fg_value < 40.0 {
            "FEAR"
        } else if fg_value < 60.0 {
            "NEUTRAL"
        } else if fg_value < 80.0 {
            "GREED"
        } else {
            "EXTREME_GREED"
        }
    }
}

impl Default for FearGreedClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fear_greed_to_gold_score() {
        // Extreme fear = bullish gold
        assert!(FearGreedClient::fear_greed_to_gold_score(10.0) > 80.0);
        // Neutral
        assert!((FearGreedClient::fear_greed_to_gold_score(50.0) - 50.0).abs() < 5.0);
        // Extreme greed = bearish gold
        assert!(FearGreedClient::fear_greed_to_gold_score(90.0) < 20.0);
    }

    #[test]
    fn test_get_category() {
        assert_eq!(FearGreedClient::get_category(10.0), "EXTREME_FEAR");
        assert_eq!(FearGreedClient::get_category(30.0), "FEAR");
        assert_eq!(FearGreedClient::get_category(50.0), "NEUTRAL");
        assert_eq!(FearGreedClient::get_category(70.0), "GREED");
        assert_eq!(FearGreedClient::get_category(90.0), "EXTREME_GREED");
    }
}
