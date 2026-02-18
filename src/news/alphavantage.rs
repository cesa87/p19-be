use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use std::path::PathBuf;
use tokio::fs;

const AV_BASE_URL: &str = "https://www.alphavantage.co/query";

/// Cache duration for Alpha Vantage news (4 hours to stay well within 25 req/day free tier)
/// 4h = 6 requests/day max (24h / 4h), leaves buffer for manual refreshes
const CACHE_DURATION_SECS: i64 = 14400;

#[derive(Debug, Clone)]
pub struct AlphaVantageClient {
    client: Client,
    api_key: String,
    cache: Arc<RwLock<Option<(AlphaVantageNewsResponse, DateTime<Utc>)>>>,
    cache_file: PathBuf,
}

/// A single news article from Alpha Vantage with sentiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvNewsArticle {
    pub title: String,
    pub url: String,
    pub summary: String,
    pub source: String,
    pub published_at: String,
    pub overall_sentiment_score: f64,
    pub overall_sentiment_label: String,
    /// Ticker-specific sentiments (e.g., FOREX:XAU, CRYPTO:BTC)
    pub ticker_sentiments: Vec<AvTickerSentiment>,
    /// Topic relevance tags
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvTickerSentiment {
    pub ticker: String,
    pub relevance_score: f64,
    pub sentiment_score: f64,
    pub sentiment_label: String,
}

/// Aggregate sentiment for MRATE consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsSentiment {
    /// Overall market sentiment from news (-1.0 to 1.0, negative=bearish, positive=bullish)
    pub overall_score: f64,
    /// Gold-specific sentiment (-1.0 to 1.0)
    pub gold_sentiment: Option<f64>,
    /// Crypto/BTC-specific sentiment (-1.0 to 1.0)
    pub crypto_sentiment: Option<f64>,
    /// Economy/Fed-related sentiment (-1.0 to 1.0)
    pub economy_sentiment: Option<f64>,
    /// Number of articles analyzed
    pub articles_analyzed: usize,
    /// Whether there's a strong bearish consensus (>60% negative articles)
    pub bearish_consensus: bool,
    /// Whether there's a strong bullish consensus (>60% positive articles)
    pub bullish_consensus: bool,
    /// Sentiment dispersion (std dev) — high = conflicting signals = uncertainty
    pub sentiment_dispersion: f64,
}

/// Raw API response from Alpha Vantage NEWS_SENTIMENT
#[derive(Debug, Clone, Deserialize)]
struct RawAvResponse {
    #[serde(default)]
    feed: Vec<RawAvArticle>,
    #[serde(default)]
    items: Option<String>,
    #[serde(default)]
    sentiment_score_definition: Option<String>,
    /// Error message if rate limited or invalid key
    #[serde(rename = "Note")]
    note: Option<String>,
    #[serde(rename = "Information")]
    information: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAvArticle {
    title: Option<String>,
    url: Option<String>,
    summary: Option<String>,
    source: Option<String>,
    time_published: Option<String>,
    overall_sentiment_score: Option<f64>,
    overall_sentiment_label: Option<String>,
    #[serde(default)]
    ticker_sentiment: Vec<RawTickerSentiment>,
    #[serde(default)]
    topics: Vec<RawTopic>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTickerSentiment {
    ticker: Option<String>,
    relevance_score: Option<String>,
    ticker_sentiment_score: Option<String>,
    ticker_sentiment_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTopic {
    topic: Option<String>,
    relevance_score: Option<String>,
}

/// Internal cached response
#[derive(Debug, Clone)]
struct AlphaVantageNewsResponse {
    articles: Vec<AvNewsArticle>,
    sentiment: NewsSentiment,
}

/// Serializable cache format for disk persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedData {
    articles: Vec<AvNewsArticle>,
    sentiment: NewsSentiment,
    timestamp: String,
}

impl AlphaVantageClient {
    pub fn new(api_key: String) -> Self {
        // Use /tmp for cache file (persistent across runs but cleared on reboot)
        let cache_file = PathBuf::from("/tmp/aureum_av_news_cache.json");
        
        let mut client = Self {
            client: Client::new(),
            api_key,
            cache: Arc::new(RwLock::new(None)),
            cache_file,
        };
        
        // Try to load cache from disk on startup
        let cache_file_clone = client.cache_file.clone();
        let cache_clone = client.cache.clone();
        tokio::spawn(async move {
            if let Ok(data) = fs::read_to_string(&cache_file_clone).await {
                if let Ok(cached) = serde_json::from_str::<CachedData>(&data) {
                    let cached_at = DateTime::parse_from_rfc3339(&cached.timestamp)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc));
                    
                    if let Some(cached_at) = cached_at {
                        if (Utc::now() - cached_at).num_seconds() < CACHE_DURATION_SECS {
                            let response = AlphaVantageNewsResponse {
                                articles: cached.articles,
                                sentiment: cached.sentiment,
                            };
                            let mut cache = cache_clone.write().await;
                            *cache = Some((response, cached_at));
                            info!("Loaded Alpha Vantage cache from disk (age: {}min)", 
                                (Utc::now() - cached_at).num_minutes());
                        }
                    }
                }
            }
        });
        
        client
    }

    /// Fetch market news with sentiment scores
    /// Covers: forex (gold, EUR, JPY), crypto (BTC), economy, finance
    pub async fn get_news_with_sentiment(&self) -> Result<(Vec<AvNewsArticle>, NewsSentiment), String> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some((cached, cached_at)) = cache.as_ref() {
                if (Utc::now() - *cached_at).num_seconds() < CACHE_DURATION_SECS {
                    return Ok((cached.articles.clone(), cached.sentiment.clone()));
                }
            }
        }

        // Fetch fresh data — use ETF equivalents for commodities/forex
        // (NEWS_SENTIMENT doesn't support FOREX: prefix, only stock tickers and CRYPTO:)
        // GLD=gold, USO=oil, SPY=S&P500, UUP=USD index, CRYPTO:BTC=bitcoin
        let url = format!(
            "{}?function=NEWS_SENTIMENT&topics=economy_monetary,finance,financial_markets,energy_transportation&tickers=GLD,CRYPTO:BTC,USO,SPY&sort=LATEST&apikey={}",
            AV_BASE_URL, self.api_key
        );

        info!("Fetching news sentiment from Alpha Vantage");

        let response = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Alpha Vantage request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Alpha Vantage error: {} - {}", status, text);
            return Err(format!("Alpha Vantage API error: {}", status));
        }

        let raw: RawAvResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Alpha Vantage response: {}", e))?;

        // Check for rate limit / error messages
        if let Some(note) = &raw.note {
            warn!("Alpha Vantage rate limit: {}", note);
            return Err("Alpha Vantage rate limit reached".to_string());
        }
        if let Some(info) = &raw.information {
            warn!("Alpha Vantage info: {}", info);
            return Err(format!("Alpha Vantage: {}", info));
        }

        // Parse articles
        let articles: Vec<AvNewsArticle> = raw.feed.into_iter().filter_map(|raw_article| {
            let title = raw_article.title?;
            let sentiment_score = raw_article.overall_sentiment_score.unwrap_or(0.0);

            let ticker_sentiments: Vec<AvTickerSentiment> = raw_article.ticker_sentiment
                .into_iter()
                .filter_map(|ts| {
                    Some(AvTickerSentiment {
                        ticker: ts.ticker?,
                        relevance_score: ts.relevance_score
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0),
                        sentiment_score: ts.ticker_sentiment_score
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0),
                        sentiment_label: ts.ticker_sentiment_label.unwrap_or_default(),
                    })
                })
                .collect();

            let topics: Vec<String> = raw_article.topics
                .into_iter()
                .filter_map(|t| t.topic)
                .collect();

            Some(AvNewsArticle {
                title,
                url: raw_article.url.unwrap_or_default(),
                summary: raw_article.summary.unwrap_or_default(),
                source: raw_article.source.unwrap_or_default(),
                published_at: raw_article.time_published.unwrap_or_default(),
                overall_sentiment_score: sentiment_score,
                overall_sentiment_label: raw_article.overall_sentiment_label.unwrap_or_default(),
                ticker_sentiments,
                topics,
            })
        }).collect();

        // Calculate aggregate sentiment
        let sentiment = Self::calculate_aggregate_sentiment(&articles);

        info!(
            "Alpha Vantage: {} articles, overall={:.3}, gold={:?}, crypto={:?}, economy={:?}, dispersion={:.3}",
            articles.len(),
            sentiment.overall_score,
            sentiment.gold_sentiment,
            sentiment.crypto_sentiment,
            sentiment.economy_sentiment,
            sentiment.sentiment_dispersion,
        );

        // Update cache (in-memory + disk)
        let now = Utc::now();
        {
            let mut cache = self.cache.write().await;
            *cache = Some((AlphaVantageNewsResponse {
                articles: articles.clone(),
                sentiment: sentiment.clone(),
            }, now));
        }
        
        // Persist to disk (async, fire-and-forget)
        let cache_file = self.cache_file.clone();
        let cached_data = CachedData {
            articles: articles.clone(),
            sentiment: sentiment.clone(),
            timestamp: now.to_rfc3339(),
        };
        tokio::spawn(async move {
            if let Ok(json) = serde_json::to_string(&cached_data) {
                let _ = fs::write(&cache_file, json).await;
            }
        });

        Ok((articles, sentiment))
    }

    /// Calculate aggregate sentiment across all articles
    fn calculate_aggregate_sentiment(articles: &[AvNewsArticle]) -> NewsSentiment {
        if articles.is_empty() {
            return NewsSentiment {
                overall_score: 0.0,
                gold_sentiment: None,
                crypto_sentiment: None,
                economy_sentiment: None,
                articles_analyzed: 0,
                bearish_consensus: false,
                bullish_consensus: false,
                sentiment_dispersion: 0.0,
            };
        }

        let n = articles.len() as f64;

        // Overall sentiment (average of all article scores)
        let overall_scores: Vec<f64> = articles.iter()
            .map(|a| a.overall_sentiment_score)
            .collect();
        let overall_score = overall_scores.iter().sum::<f64>() / n;

        // Sentiment dispersion (std deviation)
        let variance = overall_scores.iter()
            .map(|s| (s - overall_score).powi(2))
            .sum::<f64>() / n;
        let sentiment_dispersion = variance.sqrt();

        // Consensus detection
        let bearish_count = overall_scores.iter().filter(|&&s| s < -0.15).count();
        let bullish_count = overall_scores.iter().filter(|&&s| s > 0.15).count();
        let bearish_consensus = bearish_count as f64 / n > 0.6;
        let bullish_consensus = bullish_count as f64 / n > 0.6;

        // Gold-specific sentiment (from ticker_sentiment matching GLD ETF)
        let gold_sentiments: Vec<f64> = articles.iter()
            .flat_map(|a| a.ticker_sentiments.iter())
            .filter(|ts| ts.ticker == "GLD" || ts.ticker.contains("GOLD") || ts.ticker.contains("XAU"))
            .filter(|ts| ts.relevance_score > 0.1)
            .map(|ts| ts.sentiment_score)
            .collect();
        let gold_sentiment = if !gold_sentiments.is_empty() {
            Some(gold_sentiments.iter().sum::<f64>() / gold_sentiments.len() as f64)
        } else {
            None
        };

        // Crypto/BTC-specific sentiment
        let crypto_sentiments: Vec<f64> = articles.iter()
            .flat_map(|a| a.ticker_sentiments.iter())
            .filter(|ts| ts.ticker.contains("BTC") || ts.ticker.contains("CRYPTO"))
            .filter(|ts| ts.relevance_score > 0.1)
            .map(|ts| ts.sentiment_score)
            .collect();
        let crypto_sentiment = if !crypto_sentiments.is_empty() {
            Some(crypto_sentiments.iter().sum::<f64>() / crypto_sentiments.len() as f64)
        } else {
            None
        };

        // Economy/monetary policy sentiment (from topic tags)
        let economy_sentiments: Vec<f64> = articles.iter()
            .filter(|a| a.topics.iter().any(|t| {
                t.contains("Economy") || t.contains("Monetary") || t.contains("Financial")
            }))
            .map(|a| a.overall_sentiment_score)
            .collect();
        let economy_sentiment = if !economy_sentiments.is_empty() {
            Some(economy_sentiments.iter().sum::<f64>() / economy_sentiments.len() as f64)
        } else {
            None
        };

        NewsSentiment {
            overall_score,
            gold_sentiment,
            crypto_sentiment,
            economy_sentiment,
            articles_analyzed: articles.len(),
            bearish_consensus,
            bullish_consensus,
            sentiment_dispersion,
        }
    }

    /// Get just the aggregate sentiment (for MRATE engine)
    pub async fn get_sentiment(&self) -> Option<NewsSentiment> {
        match self.get_news_with_sentiment().await {
            Ok((_, sentiment)) => {
                if sentiment.articles_analyzed > 0 {
                    Some(sentiment)
                } else {
                    None
                }
            }
            Err(e) => {
                warn!("Alpha Vantage sentiment fetch failed: {}", e);
                None
            }
        }
    }
}
