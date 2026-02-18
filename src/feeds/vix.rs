//! VIX (CBOE Volatility Index) Client
//! 
//! Fetches VIX data from Yahoo Finance API

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};

/// VIX data client
pub struct VixClient {
    client: Client,
}

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooResult>>,
}

#[derive(Debug, Deserialize)]
struct YahooResult {
    meta: YahooMeta,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
}

impl VixClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
    
    /// Fetch current VIX level
    /// Returns VIX value (typically 10-80 range)
    pub async fn get_vix(&self) -> Option<f64> {
        let url = "https://query1.finance.yahoo.com/v8/finance/chart/%5EVIX?interval=1d&range=1d";
        
        match self.client.get(url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(data) = response.json::<YahooResponse>().await {
                    if let Some(results) = data.chart.result {
                        if let Some(first) = results.first() {
                            if let Some(price) = first.meta.regular_market_price {
                                info!("VIX fetched: {:.2}", price);
                                return Some(price);
                            }
                        }
                    }
                }
                warn!("Failed to parse VIX response");
                None
            }
            Err(e) => {
                warn!("Failed to fetch VIX: {}", e);
                None
            }
        }
    }
    
    /// Convert VIX level to fear score (0-100)
    /// VIX 12 = low fear (20), VIX 30 = high fear (80), VIX 50+ = extreme fear (100)
    pub fn vix_to_fear_score(vix: f64) -> f64 {
        // Linear mapping: VIX 10 -> 0, VIX 40 -> 100
        ((vix - 10.0) / 30.0 * 100.0).clamp(0.0, 100.0)
    }
}

impl Default for VixClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vix_to_fear_score() {
        assert!((VixClient::vix_to_fear_score(10.0) - 0.0).abs() < 0.1);
        assert!((VixClient::vix_to_fear_score(25.0) - 50.0).abs() < 0.1);
        assert!((VixClient::vix_to_fear_score(40.0) - 100.0).abs() < 0.1);
        assert!((VixClient::vix_to_fear_score(50.0) - 100.0).abs() < 0.1); // Clamped
    }
}
