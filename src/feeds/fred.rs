//! FRED (Federal Reserve Economic Data) Client
//! 
//! Fetches Treasury yields and economic indicators from FRED API
//! Note: FRED API is free but requires an API key from https://fred.stlouisfed.org/docs/api/api_key.html

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};

/// FRED API client
pub struct FredClient {
    client: Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FredResponse {
    observations: Option<Vec<FredObservation>>,
}

#[derive(Debug, Deserialize)]
struct FredObservation {
    date: String,
    value: String,
}

/// Treasury yield data
#[derive(Debug, Clone)]
pub struct TreasuryData {
    /// 10-Year Treasury yield (%)
    pub yield_10y: Option<f64>,
    /// 10-Year TIPS yield (real yield) (%)
    pub real_yield_10y: Option<f64>,
    /// 2-Year Treasury yield (%)
    pub yield_2y: Option<f64>,
}

impl FredClient {
    pub fn new() -> Self {
        // Try to get API key from environment
        let api_key = std::env::var("FRED_API_KEY").ok();
        if api_key.is_none() {
            warn!("FRED_API_KEY not set - Treasury yield data will be unavailable");
        }
        
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            api_key,
        }
    }
    
    /// Fetch latest observation for a FRED series
    async fn get_series(&self, series_id: &str) -> Option<f64> {
        let api_key = self.api_key.as_ref()?;
        
        let url = format!(
            "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&sort_order=desc&limit=1",
            series_id, api_key
        );
        
        match self.client.get(&url).send().await {
            Ok(response) => {
                if let Ok(data) = response.json::<FredResponse>().await {
                    if let Some(obs) = data.observations {
                        if let Some(latest) = obs.first() {
                            if let Ok(value) = latest.value.parse::<f64>() {
                                return Some(value);
                            }
                        }
                    }
                }
                None
            }
            Err(e) => {
                warn!("Failed to fetch FRED series {}: {}", series_id, e);
                None
            }
        }
    }
    
    /// Fetch all Treasury yield data
    pub async fn get_treasury_data(&self) -> TreasuryData {
        if self.api_key.is_none() {
            return TreasuryData {
                yield_10y: None,
                real_yield_10y: None,
                yield_2y: None,
            };
        }
        
        // Fetch in parallel
        let (yield_10y, real_yield, yield_2y) = tokio::join!(
            self.get_series("DGS10"),      // 10-Year Treasury Constant Maturity
            self.get_series("DFII10"),     // 10-Year TIPS (Real Yield)
            self.get_series("DGS2"),       // 2-Year Treasury
        );
        
        if yield_10y.is_some() || real_yield.is_some() {
            info!(
                "Treasury yields fetched: 10Y={:?}%, Real={:?}%, 2Y={:?}%",
                yield_10y, real_yield, yield_2y
            );
        }
        
        TreasuryData {
            yield_10y,
            real_yield_10y: real_yield,
            yield_2y,
        }
    }
    
    /// Convert real yield to gold sentiment score
    /// Negative real yields = bullish gold (score 70-100)
    /// Positive real yields = bearish gold (score 0-30)
    pub fn real_yield_to_gold_score(real_yield: f64) -> f64 {
        // Real yield -1% = 80, 0% = 50, +2% = 0
        (50.0 - real_yield * 25.0).clamp(0.0, 100.0)
    }
    
    /// Calculate yield curve slope (10Y - 2Y)
    /// Inverted curve (negative) = recession signal
    pub fn yield_curve_slope(yield_10y: f64, yield_2y: f64) -> f64 {
        yield_10y - yield_2y
    }
}

impl Default for FredClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_real_yield_to_gold_score() {
        // Negative real yields = bullish gold
        assert!(FredClient::real_yield_to_gold_score(-1.0) > 70.0);
        // Neutral
        assert!((FredClient::real_yield_to_gold_score(0.0) - 50.0).abs() < 1.0);
        // Positive real yields = bearish gold
        assert!(FredClient::real_yield_to_gold_score(2.0) < 30.0);
    }
}
