use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use std::collections::HashMap;
use std::sync::RwLock;
use chrono::{DateTime, Utc, Duration};

const GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";

/// Cache entry for market data
#[derive(Debug)]
struct MarketCache {
    markets: Vec<Market>,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct PolymarketClient {
    client: Client,
    /// Cached market data with TTL
    cache: RwLock<Option<MarketCache>>,
    /// Cache TTL in seconds (default: 60)
    cache_ttl_seconds: i64,
}

/// A Polymarket prediction market
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub condition_id: String,
    pub question: String,
    pub description: Option<String>,
    pub outcomes: Vec<String>,
    pub outcome_prices: Vec<f64>,
    pub volume: f64,
    pub liquidity: f64,
    pub end_date: Option<String>,
    pub active: bool,
    pub closed: bool,
    pub category: Option<String>,
    pub tags: Vec<String>,
}

/// Raw market response from Gamma API
#[derive(Debug, Deserialize)]
struct RawMarket {
    #[serde(rename = "conditionId")]
    condition_id: Option<String>,
    question: Option<String>,
    description: Option<String>,
    outcomes: Option<String>,  // JSON string like "[\"Yes\",\"No\"]"
    #[serde(rename = "outcomePrices")]
    outcome_prices: Option<String>,  // JSON string like "[\"0.65\",\"0.35\"]"
    volume: Option<String>,
    liquidity: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    active: Option<bool>,
    closed: Option<bool>,
    category: Option<String>,
    tags: Option<Vec<String>>,
}

/// Aggregated macro sentiment from Polymarket data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroSentiment {
    /// Overall confidence modifier (0.5 = bearish, 1.0 = neutral, 1.5 = bullish)
    pub confidence_modifier: f64,
    
    /// Fed-related metrics
    pub fed_rate_cut_probability: f64,      // 0-1, probability of rate cut at next meeting
    pub fed_rate_hike_probability: f64,     // 0-1, probability of rate hike
    pub fed_uncertainty: f64,               // 0-1, how uncertain the Fed outcome is
    
    /// Bitcoin-specific sentiment
    pub btc_bullish_probability: f64,       // 0-1, probability of BTC price increase
    pub btc_sentiment_score: f64,           // -1 to +1, overall BTC sentiment
    
    /// Gold-relevant metrics
    pub inflation_expectation: f64,         // Higher = more bullish for gold
    pub safe_haven_demand: f64,             // 0-1, proxy for risk-off sentiment
    
    /// Event risk
    pub event_blackout: bool,               // True if major event imminent
    pub next_major_event: Option<String>,   // Description of next major event
    pub hours_to_event: Option<f64>,        // Hours until next major event
    
    /// Data freshness
    pub last_updated: String,
    pub markets_analyzed: usize,
}

impl Default for MacroSentiment {
    fn default() -> Self {
        Self {
            confidence_modifier: 1.0,
            fed_rate_cut_probability: 0.0,
            fed_rate_hike_probability: 0.0,
            fed_uncertainty: 0.5,
            btc_bullish_probability: 0.5,
            btc_sentiment_score: 0.0,
            inflation_expectation: 0.5,
            safe_haven_demand: 0.5,
            event_blackout: false,
            next_major_event: None,
            hours_to_event: None,
            last_updated: chrono::Utc::now().to_rfc3339(),
            markets_analyzed: 0,
        }
    }
}

impl PolymarketClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            cache: RwLock::new(None),
            cache_ttl_seconds: 300, // 5 minute cache (Polymarket data changes slowly)
        }
    }
    
    /// Create client with custom cache TTL
    pub fn with_cache_ttl(mut self, ttl_seconds: i64) -> Self {
        self.cache_ttl_seconds = ttl_seconds;
        self
    }

    /// Fetch all markets with caching
    /// Returns cached data if within TTL, otherwise fetches fresh data
    pub async fn fetch_all_markets(&self) -> Result<Vec<Market>, String> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref cached) = *cache {
                let age = Utc::now() - cached.fetched_at;
                if age < Duration::seconds(self.cache_ttl_seconds) {
                    info!("Using cached Polymarket data ({} seconds old)", age.num_seconds());
                    return Ok(cached.markets.clone());
                }
            }
        }
        
        // Cache miss or stale - fetch fresh data
        match self.fetch_markets_uncached().await {
            Ok(markets) => {
                // Update cache with fresh data
                {
                    let mut cache = self.cache.write().unwrap();
                    *cache = Some(MarketCache {
                        markets: markets.clone(),
                        fetched_at: Utc::now(),
                    });
                }
                Ok(markets)
            }
            Err(e) => {
                // Stale fallback: return old cache on error (better than nothing)
                let cache = self.cache.read().unwrap();
                if let Some(ref cached) = *cache {
                    let age = Utc::now() - cached.fetched_at;
                    warn!(
                        "Polymarket fetch failed ({}), using stale cache ({} seconds old)",
                        e, age.num_seconds()
                    );
                    Ok(cached.markets.clone())
                } else {
                    Err(e)
                }
            }
        }
    }
    
    /// Fetch markets without caching (internal use)
    async fn fetch_markets_uncached(&self) -> Result<Vec<Market>, String> {
        let url = format!(
            "{}/markets?limit=500&active=true&closed=false&order=volume&ascending=false",
            GAMMA_API_URL
        );

        info!("Fetching fresh Polymarket markets (limit=500)");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch Polymarket data: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Polymarket API error: {} - {}", status, text);
            return Err(format!("Polymarket API error: {}", status));
        }

        let raw_markets: Vec<RawMarket> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Polymarket response: {}", e))?;

        let markets: Vec<Market> = raw_markets
            .into_iter()
            .filter_map(|raw| self.parse_market(raw))
            .collect();

        info!("Fetched {} total markets from Polymarket", markets.len());
        Ok(markets)
    }
    
    /// Invalidate the cache (force fresh fetch on next call)
    pub fn invalidate_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        *cache = None;
    }

    /// Fetch Fed-related markets
    /// 
    /// **Deprecated**: This method fetches all markets. For efficiency, prefer using
    /// `calculate_macro_sentiment()` which fetches once and filters internally.
    #[deprecated(since = "0.2.0", note = "Use calculate_macro_sentiment() instead for efficiency")]
    pub async fn get_fed_markets(&self) -> Result<Vec<Market>, String> {
        let all_markets = self.fetch_all_markets().await?;
        
        let markets: Vec<Market> = all_markets
            .into_iter()
            .filter(|m| {
                let q = m.question.to_lowercase();
                // Fed-specific terms (avoid matching "federal" in "federal spending")
                q.contains("federal reserve") || 
                q.contains("fomc") || 
                q.contains("interest rate") ||
                q.contains("rate cut") ||
                q.contains("rate hike") ||
                q.contains("powell") ||
                (q.contains("fed") && (q.contains("rate") || q.contains("monetary")))
            })
            .collect();

        info!("Found {} Fed-related markets", markets.len());
        Ok(markets)
    }

    /// Fetch Bitcoin-related markets
    /// 
    /// **Deprecated**: This method fetches all markets. For efficiency, prefer using
    /// `calculate_macro_sentiment()` which fetches once and filters internally.
    #[deprecated(since = "0.2.0", note = "Use calculate_macro_sentiment() instead for efficiency")]
    pub async fn get_btc_markets(&self) -> Result<Vec<Market>, String> {
        let all_markets = self.fetch_all_markets().await?;
        
        let markets: Vec<Market> = all_markets
            .into_iter()
            .filter(|m| {
                let q = m.question.to_lowercase();
                // Any BTC/Bitcoin market (not just price ones)
                q.contains("bitcoin") || q.contains("btc ")
            })
            .collect();

        info!("Found {} BTC-related markets", markets.len());
        Ok(markets)
    }

    /// Fetch inflation/economic markets
    /// 
    /// **Deprecated**: This method fetches all markets. For efficiency, prefer using
    /// `calculate_macro_sentiment()` which fetches once and filters internally.
    #[deprecated(since = "0.2.0", note = "Use calculate_macro_sentiment() instead for efficiency")]
    pub async fn get_economic_markets(&self) -> Result<Vec<Market>, String> {
        let all_markets = self.fetch_all_markets().await?;
        
        let markets: Vec<Market> = all_markets
            .into_iter()
            .filter(|m| {
                let q = m.question.to_lowercase();
                q.contains("inflation") || 
                q.contains("cpi") || 
                q.contains("recession") || 
                q.contains("gdp") ||
                q.contains("treasury") ||
                q.contains("economy") ||
                q.contains("tariff")
            })
            .collect();

        info!("Found {} economic markets", markets.len());
        Ok(markets)
    }

    /// Calculate aggregated macro sentiment from all relevant markets
    pub async fn calculate_macro_sentiment(&self) -> MacroSentiment {
        let mut sentiment = MacroSentiment::default();

        // Fetch all markets once
        let all_markets = match self.fetch_all_markets().await {
            Ok(markets) => markets,
            Err(e) => {
                warn!("Failed to fetch Polymarket data: {}", e);
                return sentiment;
            }
        };

        // Filter into categories
        let fed_markets: Vec<&Market> = all_markets.iter().filter(|m| {
            let q = m.question.to_lowercase();
            q.contains("federal reserve") || 
            q.contains("fomc") || 
            q.contains("interest rate") ||
            q.contains("rate cut") ||
            q.contains("rate hike") ||
            q.contains("powell") ||
            (q.contains("fed") && (q.contains("rate") || q.contains("monetary")))
        }).collect();

        let btc_markets: Vec<&Market> = all_markets.iter().filter(|m| {
            let q = m.question.to_lowercase();
            q.contains("bitcoin") || q.contains("btc ")
        }).collect();

        let econ_markets: Vec<&Market> = all_markets.iter().filter(|m| {
            let q = m.question.to_lowercase();
            q.contains("inflation") || 
            q.contains("cpi") || 
            q.contains("recession") || 
            q.contains("gdp") ||
            q.contains("treasury") ||
            q.contains("economy") ||
            q.contains("tariff")
        }).collect();

        let markets_count = fed_markets.len() + btc_markets.len() + econ_markets.len();
        
        info!("Found markets: {} Fed, {} BTC, {} Econ", 
            fed_markets.len(), btc_markets.len(), econ_markets.len());

        // Analyze each category
        self.analyze_fed_sentiment_refs(&fed_markets, &mut sentiment);
        self.analyze_btc_sentiment_refs(&btc_markets, &mut sentiment);
        self.analyze_economic_sentiment_refs(&econ_markets, &mut sentiment);

        // Calculate overall confidence modifier
        sentiment.confidence_modifier = self.calculate_confidence_modifier(&sentiment);
        sentiment.markets_analyzed = markets_count;
        sentiment.last_updated = chrono::Utc::now().to_rfc3339();

        info!(
            "Macro sentiment calculated: modifier={:.2}, fed_uncertainty={:.2}, btc_sentiment={:.2}",
            sentiment.confidence_modifier,
            sentiment.fed_uncertainty,
            sentiment.btc_sentiment_score
        );

        sentiment
    }

    fn parse_market(&self, raw: RawMarket) -> Option<Market> {
        let condition_id = raw.condition_id?;
        let question = raw.question.unwrap_or_default();
        
        // Parse outcomes JSON string
        let outcomes: Vec<String> = raw.outcomes
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| vec!["Yes".to_string(), "No".to_string()]);
        
        // Parse outcome prices JSON string
        let outcome_prices: Vec<f64> = raw.outcome_prices
            .and_then(|s| {
                let strings: Vec<String> = serde_json::from_str(&s).ok()?;
                Some(strings.iter().filter_map(|p| p.parse().ok()).collect())
            })
            .unwrap_or_else(|| vec![0.5, 0.5]);

        let volume = raw.volume
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        let liquidity = raw.liquidity
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        Some(Market {
            condition_id,
            question,
            description: raw.description,
            outcomes,
            outcome_prices,
            volume,
            liquidity,
            end_date: raw.end_date,
            active: raw.active.unwrap_or(true),
            closed: raw.closed.unwrap_or(false),
            category: raw.category,
            tags: raw.tags.unwrap_or_default(),
        })
    }

    fn analyze_fed_sentiment(&self, markets: &[Market], sentiment: &mut MacroSentiment) {
        let mut rate_cut_probs = Vec::new();
        let mut rate_hike_probs = Vec::new();
        let mut no_change_probs = Vec::new();

        for market in markets {
            let q = market.question.to_lowercase();
            
            // Look for rate decision markets
            if q.contains("decision") || q.contains("rate") {
                for (i, outcome) in market.outcomes.iter().enumerate() {
                    let outcome_lower = outcome.to_lowercase();
                    let price = market.outcome_prices.get(i).copied().unwrap_or(0.0);
                    
                    if outcome_lower.contains("cut") || outcome_lower.contains("decrease") {
                        rate_cut_probs.push(price);
                    } else if outcome_lower.contains("hike") || outcome_lower.contains("increase") {
                        rate_hike_probs.push(price);
                    } else if outcome_lower.contains("no change") || outcome_lower.contains("pause") {
                        no_change_probs.push(price);
                    }
                }
            }

            // Look for rate cut specific markets
            if q.contains("rate cut") && !q.contains("no") {
                if let Some(yes_price) = market.outcome_prices.first() {
                    rate_cut_probs.push(*yes_price);
                }
            }

            // Look for rate hike specific markets
            if q.contains("rate hike") && !q.contains("no") {
                if let Some(yes_price) = market.outcome_prices.first() {
                    rate_hike_probs.push(*yes_price);
                }
            }
        }

        // Average the probabilities
        if !rate_cut_probs.is_empty() {
            sentiment.fed_rate_cut_probability = 
                rate_cut_probs.iter().sum::<f64>() / rate_cut_probs.len() as f64;
        }

        if !rate_hike_probs.is_empty() {
            sentiment.fed_rate_hike_probability = 
                rate_hike_probs.iter().sum::<f64>() / rate_hike_probs.len() as f64;
        }

        // Calculate uncertainty (high when no clear winner)
        let max_prob = sentiment.fed_rate_cut_probability
            .max(sentiment.fed_rate_hike_probability)
            .max(if no_change_probs.is_empty() { 0.0 } else { 
                no_change_probs.iter().sum::<f64>() / no_change_probs.len() as f64 
            });
        
        // Uncertainty is high when max_prob is around 0.5, low when near 0 or 1
        sentiment.fed_uncertainty = 1.0 - (2.0 * (max_prob - 0.5).abs());
    }

    fn analyze_btc_sentiment(&self, markets: &[Market], sentiment: &mut MacroSentiment) {
        let mut bullish_signals = Vec::new();
        let mut bearish_signals = Vec::new();

        for market in markets {
            let q = market.question.to_lowercase();
            
            // Look for price target markets
            if q.contains("above") || q.contains("reach") || q.contains("hit") {
                // "Will BTC hit $X?" - Yes price is bullish probability
                if let Some(yes_price) = market.outcome_prices.first() {
                    bullish_signals.push(*yes_price);
                }
            } else if q.contains("below") || q.contains("drop") || q.contains("fall") {
                // "Will BTC drop below $X?" - Yes price is bearish probability
                if let Some(yes_price) = market.outcome_prices.first() {
                    bearish_signals.push(*yes_price);
                }
            }
        }

        // Calculate bullish probability
        if !bullish_signals.is_empty() {
            sentiment.btc_bullish_probability = 
                bullish_signals.iter().sum::<f64>() / bullish_signals.len() as f64;
        }

        // Calculate sentiment score (-1 to +1)
        let avg_bullish = if bullish_signals.is_empty() { 0.5 } else {
            bullish_signals.iter().sum::<f64>() / bullish_signals.len() as f64
        };
        let avg_bearish = if bearish_signals.is_empty() { 0.5 } else {
            bearish_signals.iter().sum::<f64>() / bearish_signals.len() as f64
        };
        
        // Sentiment = bullish - bearish, normalized
        sentiment.btc_sentiment_score = (avg_bullish - avg_bearish).clamp(-1.0, 1.0);
    }

    fn analyze_economic_sentiment(&self, markets: &[Market], sentiment: &mut MacroSentiment) {
        let mut inflation_signals = Vec::new();
        let mut recession_signals = Vec::new();

        for market in markets {
            let q = market.question.to_lowercase();
            
            // Inflation markets
            if q.contains("inflation") || q.contains("cpi") {
                if q.contains("higher") || q.contains("above") || q.contains("increase") {
                    if let Some(yes_price) = market.outcome_prices.first() {
                        inflation_signals.push(*yes_price);
                    }
                }
            }

            // Recession markets (proxy for safe haven demand)
            if q.contains("recession") {
                if let Some(yes_price) = market.outcome_prices.first() {
                    recession_signals.push(*yes_price);
                }
            }
        }

        // Higher inflation expectation = bullish for gold
        if !inflation_signals.is_empty() {
            sentiment.inflation_expectation = 
                inflation_signals.iter().sum::<f64>() / inflation_signals.len() as f64;
        }

        // Recession probability = safe haven demand
        if !recession_signals.is_empty() {
            sentiment.safe_haven_demand = 
                recession_signals.iter().sum::<f64>() / recession_signals.len() as f64;
        }
    }

    // Reference versions for use with filtered slices
    fn analyze_fed_sentiment_refs(&self, markets: &[&Market], sentiment: &mut MacroSentiment) {
        let owned: Vec<Market> = markets.iter().map(|m| (*m).clone()).collect();
        self.analyze_fed_sentiment(&owned, sentiment);
    }

    fn analyze_btc_sentiment_refs(&self, markets: &[&Market], sentiment: &mut MacroSentiment) {
        let owned: Vec<Market> = markets.iter().map(|m| (*m).clone()).collect();
        self.analyze_btc_sentiment(&owned, sentiment);
    }

    fn analyze_economic_sentiment_refs(&self, markets: &[&Market], sentiment: &mut MacroSentiment) {
        let owned: Vec<Market> = markets.iter().map(|m| (*m).clone()).collect();
        self.analyze_economic_sentiment(&owned, sentiment);
    }

    fn calculate_confidence_modifier(&self, sentiment: &MacroSentiment) -> f64 {
        let mut modifier: f64 = 1.0;

        // Fed uncertainty reduces confidence (can swing either way)
        // High uncertainty (>0.6) = reduce modifier
        if sentiment.fed_uncertainty > 0.6 {
            modifier *= 0.85;  // 15% reduction
        } else if sentiment.fed_uncertainty > 0.4 {
            modifier *= 0.95;  // 5% reduction
        }

        // Rate cut probability is bullish for Gold
        // Increases confidence for gold longs
        if sentiment.fed_rate_cut_probability > 0.6 {
            modifier *= 1.1;  // 10% boost
        } else if sentiment.fed_rate_cut_probability > 0.4 {
            modifier *= 1.05;  // 5% boost
        }

        // Rate hike probability is bearish for Gold
        if sentiment.fed_rate_hike_probability > 0.3 {
            modifier *= 0.9;  // 10% reduction
        }

        // Safe haven demand is bullish for Gold
        if sentiment.safe_haven_demand > 0.5 {
            modifier *= 1.05;  // 5% boost
        }

        // BTC sentiment affects BTC strategies
        // (This is applied selectively based on instrument)
        
        // Event blackout significantly reduces confidence
        if sentiment.event_blackout {
            modifier *= 0.5;  // 50% reduction during blackout
        }

        // Clamp to reasonable range
        modifier.clamp(0.5, 1.5)
    }
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Clone manually since RwLock doesn't implement Clone
impl Clone for PolymarketClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            cache: RwLock::new(None), // Fresh cache for clone
            cache_ttl_seconds: self.cache_ttl_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_sentiment() {
        let sentiment = MacroSentiment::default();
        assert_eq!(sentiment.confidence_modifier, 1.0);
        assert_eq!(sentiment.fed_uncertainty, 0.5);
        assert!(!sentiment.event_blackout);
    }
}
