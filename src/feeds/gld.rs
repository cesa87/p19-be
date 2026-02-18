//! GLD (SPDR Gold Shares) Volume Feed
//!
//! Fetches GLD ETF volume data from Yahoo Finance.
//! High volume relative to average indicates institutional gold flows.
//! Used in MRATE score_gold to detect large-scale gold demand/supply.

use reqwest::Client;
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Cached GLD data
struct CachedGld {
    data: Option<GldFlowData>,
    last_fetch: Instant,
}

lazy_static::lazy_static! {
    static ref GLD_CACHE: Mutex<CachedGld> = Mutex::new(CachedGld {
        data: None,
        last_fetch: Instant::now() - Duration::from_secs(3600),
    });
}

const GLD_CACHE_TTL: Duration = Duration::from_secs(900); // 15 min cache (volume doesn't change fast)

/// GLD ETF flow data
#[derive(Debug, Clone)]
pub struct GldFlowData {
    /// Current day's volume
    pub current_volume: f64,
    /// Average volume over the lookback period
    pub avg_volume: f64,
    /// Volume ratio (current / avg): >1.5 = heavy flow, <0.5 = quiet
    pub volume_ratio: f64,
}

/// Yahoo Finance response structures
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
    indicators: Option<YahooIndicators>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Option<Vec<YahooQuote>>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    volume: Option<Vec<Option<f64>>>,
}

/// GLD volume client
pub struct GldClient {
    client: Client,
}

impl GldClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch GLD volume data (with caching)
    pub async fn get_gld_flow(&self) -> Option<GldFlowData> {
        // Check cache
        {
            let cache = GLD_CACHE.lock().ok()?;
            if cache.last_fetch.elapsed() < GLD_CACHE_TTL {
                if cache.data.is_some() {
                    return cache.data.clone();
                }
            }
        }

        // Fetch 20 days of daily volume for GLD
        let url = "https://query1.finance.yahoo.com/v8/finance/chart/GLD?interval=1d&range=20d";

        match self.client
            .get(url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(data) = response.json::<YahooResponse>().await {
                    if let Some(result) = self.extract_flow_data(&data) {
                        info!(
                            "GLD volume: current={:.0}, avg={:.0}, ratio={:.2}",
                            result.current_volume, result.avg_volume, result.volume_ratio
                        );

                        // Update cache
                        if let Ok(mut cache) = GLD_CACHE.lock() {
                            cache.data = Some(result.clone());
                            cache.last_fetch = Instant::now();
                        }

                        return Some(result);
                    }
                }
                warn!("Failed to parse GLD response");
                // Return stale cache on parse failure
                GLD_CACHE.lock().ok().and_then(|c| c.data.clone())
            }
            Err(e) => {
                warn!("Failed to fetch GLD volume: {}", e);
                GLD_CACHE.lock().ok().and_then(|c| c.data.clone())
            }
        }
    }

    fn extract_flow_data(&self, response: &YahooResponse) -> Option<GldFlowData> {
        let result = response.chart.result.as_ref()?.first()?;
        let volumes: Vec<f64> = result.indicators.as_ref()?
            .quote.as_ref()?
            .first()?
            .volume.as_ref()?
            .iter()
            .filter_map(|v| *v)
            .collect();

        if volumes.len() < 5 {
            return None;
        }

        let current_volume = *volumes.last()?;
        let avg_volume = volumes.iter().sum::<f64>() / volumes.len() as f64;
        let volume_ratio = if avg_volume > 0.0 { current_volume / avg_volume } else { 1.0 };

        Some(GldFlowData {
            current_volume,
            avg_volume,
            volume_ratio,
        })
    }
}

impl Default for GldClient {
    fn default() -> Self {
        Self::new()
    }
}
