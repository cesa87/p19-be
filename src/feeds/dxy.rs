//! DXY (US Dollar Index) Client
//!
//! Fetches DXY data from Yahoo Finance API
//! DXY measures USD strength against a basket of currencies
//! Inverse correlation with gold: DXY up = Gold down

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached DXY data
struct CachedDxy {
    data: Option<DxyData>,
    last_fetch: Instant,
}

// Global cache for DXY (avoid rate limiting)
lazy_static::lazy_static! {
    static ref DXY_CACHE: Mutex<CachedDxy> = Mutex::new(CachedDxy {
        data: None,
        last_fetch: Instant::now() - Duration::from_secs(3600), // Start expired
    });
}

const DXY_CACHE_TTL: Duration = Duration::from_secs(600); // 10 minute cache

/// DXY data client
pub struct DxyClient {
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
    indicators: Option<YahooIndicators>,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "previousClose")]
    previous_close: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Option<Vec<YahooQuote>>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    close: Option<Vec<Option<f64>>>,
}

/// DXY data result
#[derive(Debug, Clone)]
pub struct DxyData {
    /// Current DXY level (typically 90-110 range)
    pub level: f64,
    /// Previous close
    pub prev_close: f64,
    /// Daily change percentage
    pub change_pct: f64,
    /// 5-day simple moving average
    pub sma_5d: Option<f64>,
}

impl DxyClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch current DXY level and recent history (with caching)
    pub async fn get_dxy(&self) -> Option<DxyData> {
        // Check cache first
        {
            let cache = DXY_CACHE.lock().ok()?;
            if cache.last_fetch.elapsed() < DXY_CACHE_TTL {
                if cache.data.is_some() {
                    info!("DXY: Using cached value");
                    return cache.data.clone();
                }
            }
        }
        
        // DX-Y.NYB is the ICE Dollar Index futures
        let url = "https://query1.finance.yahoo.com/v8/finance/chart/DX-Y.NYB?interval=1d&range=5d";

        match self.client
            .get(url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(data) = response.json::<YahooResponse>().await {
                    if let Some(results) = data.chart.result {
                        if let Some(first) = results.first() {
                            let level = first.meta.regular_market_price?;
                            let prev_close = first.meta.previous_close.unwrap_or(level);
                            let change_pct = (level - prev_close) / prev_close * 100.0;

                            // Calculate 5-day SMA
                            let sma_5d = first.indicators.as_ref().and_then(|ind| {
                                ind.quote.as_ref().and_then(|quotes| {
                                    quotes.first().and_then(|q| {
                                        q.close.as_ref().and_then(|closes| {
                                            let valid: Vec<f64> =
                                                closes.iter().filter_map(|c| *c).collect();
                                            if valid.len() >= 3 {
                                                Some(valid.iter().sum::<f64>() / valid.len() as f64)
                                            } else {
                                                None
                                            }
                                        })
                                    })
                                })
                            });

                            info!(
                                "DXY fetched: {:.2} (change: {:.2}%, 5d SMA: {:?})",
                                level, change_pct, sma_5d
                            );

                            let result = DxyData {
                                level,
                                prev_close,
                                change_pct,
                                sma_5d,
                            };
                            
                            // Update cache
                            if let Ok(mut cache) = DXY_CACHE.lock() {
                                cache.data = Some(result.clone());
                                cache.last_fetch = Instant::now();
                            }
                            
                            return Some(result);
                        }
                    }
                }
                warn!("Failed to parse DXY response");
                None
            }
            Err(e) => {
                warn!("Failed to fetch DXY: {}", e);
                // Return cached value on error
                if let Ok(cache) = DXY_CACHE.lock() {
                    if cache.data.is_some() {
                        info!("DXY: Using stale cache due to error");
                        return cache.data.clone();
                    }
                }
                None
            }
        }
    }

    /// Convert DXY level to gold sentiment score
    /// Higher DXY = bearish for gold (score 0-30)
    /// Lower DXY = bullish for gold (score 70-100)
    pub fn dxy_to_gold_score(dxy: f64) -> f64 {
        // DXY typically ranges 90-110
        // DXY 95 = bullish gold (80), DXY 100 = neutral (50), DXY 105 = bearish gold (20)
        (50.0 - (dxy - 100.0) * 6.0).clamp(0.0, 100.0)
    }

    /// Determine DXY trend based on current vs SMA
    pub fn dxy_trend(current: f64, sma: Option<f64>) -> &'static str {
        match sma {
            Some(sma) => {
                let diff_pct = (current - sma) / sma * 100.0;
                if diff_pct > 1.0 {
                    "STRONG_UP"
                } else if diff_pct > 0.3 {
                    "UP"
                } else if diff_pct < -1.0 {
                    "STRONG_DOWN"
                } else if diff_pct < -0.3 {
                    "DOWN"
                } else {
                    "FLAT"
                }
            }
            None => "UNKNOWN",
        }
    }
}

impl Default for DxyClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dxy_to_gold_score() {
        // Strong dollar = bearish gold
        assert!(DxyClient::dxy_to_gold_score(105.0) < 30.0);
        // Neutral
        assert!((DxyClient::dxy_to_gold_score(100.0) - 50.0).abs() < 5.0);
        // Weak dollar = bullish gold
        assert!(DxyClient::dxy_to_gold_score(95.0) > 70.0);
    }
}
