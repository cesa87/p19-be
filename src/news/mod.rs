pub mod finnhub;
pub mod alphavantage;
pub mod newsapi;

pub use finnhub::{FinnhubClient, EconomicEvent, NewsArticle};
pub use alphavantage::{AlphaVantageClient, AvNewsArticle, AvTickerSentiment, NewsSentiment};
pub use newsapi::{NewsApiClient, NewsAnalysis, NewsApiArticle, RegimeEvent};

use serde::Serialize;
use tracing::{info, warn};

/// Unified news article that works with both sources
#[derive(Debug, Clone, Serialize)]
pub struct UnifiedNewsArticle {
    pub title: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub timestamp: i64,
    pub provider: &'static str,
    /// Sentiment score (-1.0 to 1.0) — only available from Alpha Vantage
    pub sentiment_score: Option<f64>,
    pub sentiment_label: Option<String>,
}

/// Multi-source news client - tries NewsAPI first (best), then Alpha Vantage, then Finnhub
#[derive(Debug, Clone)]
pub struct MultiSourceNews {
    newsapi: Option<NewsApiClient>,
    alpha_vantage: Option<AlphaVantageClient>,
    finnhub: Option<FinnhubClient>,
}

impl MultiSourceNews {
    pub fn new(newsapi_key: Option<String>, av_key: Option<String>, finnhub_key: Option<String>) -> Self {
        Self {
            newsapi: newsapi_key.filter(|k| !k.is_empty()).map(NewsApiClient::new),
            alpha_vantage: av_key
                .filter(|k| !k.is_empty())
                .map(AlphaVantageClient::new),
            finnhub: finnhub_key
                .filter(|k| !k.is_empty())
                .map(FinnhubClient::new),
        }
    }

    /// Get news articles from the best available source
    /// Priority: NewsAPI (best) -> Alpha Vantage -> Finnhub
    pub async fn get_news(&self) -> Result<(Vec<UnifiedNewsArticle>, Option<NewsSentiment>), String> {
        // Try NewsAPI first (best: includes regime event detection + sentiment)
        if let Some(newsapi) = &self.newsapi {
            match newsapi.get_articles().await {
                Ok(articles) => {
                    info!("📰 News from NewsAPI: {} articles", articles.len());
                    
                    // Convert to unified format
                    let unified: Vec<UnifiedNewsArticle> = articles.iter().map(|a| {
                        let ts = parse_newsapi_timestamp(&a.published_at);
                        UnifiedNewsArticle {
                            title: a.title.clone(),
                            summary: a.description.clone(),
                            source: a.source.clone(),
                            url: a.url.clone(),
                            timestamp: ts,
                            provider: "newsapi",
                            sentiment_score: Some(a.sentiment_score),
                            sentiment_label: Some(sentiment_label(a.sentiment_score)),
                        }
                    }).collect();
                    
                    // Get analysis for sentiment
                    let analysis = newsapi.get_analysis().await;
                    let sentiment = analysis.map(|a| NewsSentiment {
                        overall_score: a.overall_sentiment,
                        gold_sentiment: Some(a.gold_sentiment),
                        crypto_sentiment: Some(a.crypto_sentiment),
                        economy_sentiment: Some(a.fed_sentiment),
                        articles_analyzed: a.articles_analyzed,
                        bearish_consensus: a.bearish_ratio > 0.6,
                        bullish_consensus: a.bullish_ratio > 0.6,
                        sentiment_dispersion: (a.bearish_ratio - a.bullish_ratio).abs(),
                    });
                    
                    return Ok((unified, sentiment));
                }
                Err(e) => {
                    warn!("NewsAPI failed: {}, trying Alpha Vantage fallback", e);
                }
            }
        }
        
        // Fallback to Alpha Vantage (has sentiment scores)
        if let Some(av) = &self.alpha_vantage {
            match av.get_news_with_sentiment().await {
                Ok((articles, sentiment)) => {
                    info!("News from Alpha Vantage: {} articles", articles.len());
                    let unified: Vec<UnifiedNewsArticle> = articles.into_iter().map(|a| {
                        let ts = parse_av_timestamp(&a.published_at);
                        UnifiedNewsArticle {
                            title: a.title,
                            summary: a.summary,
                            source: a.source,
                            url: a.url,
                            timestamp: ts,
                            provider: "alpha_vantage",
                            sentiment_score: Some(a.overall_sentiment_score),
                            sentiment_label: Some(a.overall_sentiment_label),
                        }
                    }).collect();
                    return Ok((unified, Some(sentiment)));
                }
                Err(e) => {
                    warn!("Alpha Vantage failed, trying Finnhub fallback: {}", e);
                }
            }
        }

        // Fallback to Finnhub for display articles
        if let Some(fh) = &self.finnhub {
            match fh.get_gold_news().await {
                Ok(articles) => {
                    info!("News from Finnhub (fallback): {} articles", articles.len());
                    let unified: Vec<UnifiedNewsArticle> = articles.into_iter().map(|a| {
                        UnifiedNewsArticle {
                            title: a.headline,
                            summary: a.summary,
                            source: a.source,
                            url: a.url,
                            timestamp: a.timestamp,
                            provider: "finnhub",
                            sentiment_score: None,
                            sentiment_label: None,
                        }
                    }).collect();
                    return Ok((unified, None));
                }
                Err(e) => {
                    warn!("Finnhub also failed: {}", e);
                    return Err(format!("All news sources failed. Last error: {}", e));
                }
            }
        }

        Err("No news API keys configured".to_string())
    }

    /// Get just the sentiment (for MRATE engine)
    /// Prefers NewsAPI (has regime detection), falls back to Alpha Vantage
    pub async fn get_sentiment(&self) -> Option<NewsSentiment> {
        // Try NewsAPI first (has regime event detection)
        if let Some(newsapi) = &self.newsapi {
            if let Some(analysis) = newsapi.get_analysis().await {
                return Some(NewsSentiment {
                    overall_score: analysis.overall_sentiment,
                    gold_sentiment: Some(analysis.gold_sentiment),
                    crypto_sentiment: Some(analysis.crypto_sentiment),
                    economy_sentiment: Some(analysis.fed_sentiment),
                    articles_analyzed: analysis.articles_analyzed,
                    bearish_consensus: analysis.bearish_ratio > 0.6,
                    bullish_consensus: analysis.bullish_ratio > 0.6,
                    sentiment_dispersion: (analysis.bearish_ratio - analysis.bullish_ratio).abs(),
                });
            }
        }
        
        // Fallback to Alpha Vantage
        if let Some(av) = &self.alpha_vantage {
            return av.get_sentiment().await;
        }
        
        None
    }
}

/// Parse Alpha Vantage timestamp format "20260214T143000" to Unix timestamp
fn parse_av_timestamp(ts: &str) -> i64 {
    use chrono::NaiveDateTime;
    NaiveDateTime::parse_from_str(ts, "%Y%m%dT%H%M%S")
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(0)
}

/// Parse NewsAPI timestamp format "2026-02-17T12:00:00Z" to Unix timestamp
fn parse_newsapi_timestamp(ts: &str) -> i64 {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| {
            // Try alternative format without timezone
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S")
                .map(|dt| dt.and_utc().timestamp())
                .unwrap_or(0)
        })
}

/// Convert sentiment score to label
fn sentiment_label(score: f64) -> String {
    if score >= 0.35 {
        "Bullish".to_string()
    } else if score >= 0.15 {
        "Somewhat-Bullish".to_string()
    } else if score > -0.15 {
        "Neutral".to_string()
    } else if score > -0.35 {
        "Somewhat-Bearish".to_string()
    } else {
        "Bearish".to_string()
    }
}
