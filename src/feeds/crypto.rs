//! Crypto Market Data Client
//! 
//! Fetches crypto-specific indicators:
//! - Stablecoin market dominance (CoinGecko)
//! - BTC exchange netflows (placeholder for CryptoQuant)

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};

/// Crypto data client
pub struct CryptoClient {
    client: Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoGlobal {
    data: CoinGeckoGlobalData,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoGlobalData {
    total_market_cap: std::collections::HashMap<String, f64>,
    market_cap_percentage: std::collections::HashMap<String, f64>,
}

/// Crypto market data
#[derive(Debug, Clone)]
pub struct CryptoMarketData {
    /// Stablecoin dominance as % of total crypto market cap
    /// Higher = more "dry powder" waiting to deploy
    pub stablecoin_dominance: Option<f64>,
    /// BTC dominance as % of total crypto market cap
    pub btc_dominance: Option<f64>,
    /// BTC exchange netflow (positive = inflows = bearish)
    /// Placeholder - requires CryptoQuant API
    pub btc_exchange_netflow: Option<f64>,
}

impl CryptoClient {
    pub fn new() -> Self {
        let api_key = std::env::var("COINGECKO_API_KEY").ok();
        if api_key.is_some() {
            info!("CoinGecko API key configured");
        } else {
            warn!("COINGECKO_API_KEY not set - using free tier (may be rate limited)");
        }
        
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            api_key,
        }
    }
    
    /// Fetch global crypto market data from CoinGecko
    pub async fn get_market_data(&self) -> CryptoMarketData {
        // Use demo API if we have a key, otherwise free API
        let url = if self.api_key.is_some() {
            "https://api.coingecko.com/api/v3/global"
        } else {
            "https://api.coingecko.com/api/v3/global"
        };
        
        let mut request = self.client.get(url)
            .header("Accept", "application/json");
        
        // Add API key header if available
        if let Some(key) = &self.api_key {
            request = request.header("x-cg-demo-api-key", key);
        }
        
        match request.send().await
        {
            Ok(response) => {
                if let Ok(data) = response.json::<CoinGeckoGlobal>().await {
                    // Calculate stablecoin dominance
                    // Major stablecoins: USDT, USDC, DAI, BUSD
                    let stablecoin_dom = 
                        data.data.market_cap_percentage.get("usdt").unwrap_or(&0.0) +
                        data.data.market_cap_percentage.get("usdc").unwrap_or(&0.0);
                    
                    let btc_dom = data.data.market_cap_percentage.get("btc").copied();
                    
                    info!(
                        "Crypto data fetched: Stablecoin dom={:.2}%, BTC dom={:?}%",
                        stablecoin_dom, btc_dom
                    );
                    
                    return CryptoMarketData {
                        stablecoin_dominance: Some(stablecoin_dom),
                        btc_dominance: btc_dom,
                        btc_exchange_netflow: None, // Would need CryptoQuant API
                    };
                }
                warn!("Failed to parse CoinGecko response");
                CryptoMarketData::default()
            }
            Err(e) => {
                warn!("Failed to fetch CoinGecko data: {}", e);
                CryptoMarketData::default()
            }
        }
    }
    
    /// Convert stablecoin dominance to liquidity signal
    /// High stablecoin dom (>8%) = lots of dry powder = bullish potential
    /// Low stablecoin dom (<4%) = fully deployed = less upside
    pub fn stablecoin_to_liquidity_score(dominance: f64) -> f64 {
        // 4% = 0, 6% = 50, 8% = 100
        ((dominance - 4.0) / 4.0 * 100.0).clamp(0.0, 100.0)
    }
    
    /// Convert BTC exchange netflow to risk signal
    /// Positive (inflows to exchanges) = bearish (people selling)
    /// Negative (outflows from exchanges) = bullish (people holding)
    pub fn netflow_to_risk_score(netflow_btc: f64) -> f64 {
        // -5000 BTC = 0 (very bullish), 0 = 50, +5000 = 100 (very bearish)
        ((netflow_btc / 5000.0 + 1.0) * 50.0).clamp(0.0, 100.0)
    }
}

impl Default for CryptoClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for CryptoMarketData {
    fn default() -> Self {
        Self {
            stablecoin_dominance: None,
            btc_dominance: None,
            btc_exchange_netflow: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stablecoin_to_liquidity_score() {
        assert!((CryptoClient::stablecoin_to_liquidity_score(4.0) - 0.0).abs() < 0.1);
        assert!((CryptoClient::stablecoin_to_liquidity_score(6.0) - 50.0).abs() < 0.1);
        assert!((CryptoClient::stablecoin_to_liquidity_score(8.0) - 100.0).abs() < 0.1);
    }
    
    #[test]
    fn test_netflow_to_risk_score() {
        assert!(CryptoClient::netflow_to_risk_score(-5000.0) < 10.0);  // Outflows = bullish
        assert!((CryptoClient::netflow_to_risk_score(0.0) - 50.0).abs() < 5.0);  // Neutral
        assert!(CryptoClient::netflow_to_risk_score(5000.0) > 90.0);  // Inflows = bearish
    }
}
