//! NewsAPI.org client with local sentiment analysis and regime event detection
//!
//! Free tier: 100 requests/day, which is plenty with 4-hour caching

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

const NEWSAPI_BASE_URL: &str = "https://newsapi.org/v2";
const CACHE_DURATION_SECS: i64 = 14400; // 4 hours = 6 requests/day max

/// Regime-shifting event types that require special attention
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RegimeEvent {
    /// New Fed Chair appointment - major policy shift expected
    FedChairChange,
    /// Emergency rate decision
    EmergencyRateAction,
    /// Major geopolitical escalation (war, sanctions)
    GeopoliticalEscalation,
    /// Major trade policy change (tariffs)
    TradePolicyShift,
    /// Banking crisis / financial instability
    FinancialCrisis,
    /// Significant inflation surprise
    InflationShock,
    /// No regime-shifting event detected
    None,
}

impl RegimeEvent {
    /// How much to boost uncertainty score when this event is detected
    pub fn uncertainty_boost(&self) -> f64 {
        match self {
            RegimeEvent::FedChairChange => 25.0,
            RegimeEvent::EmergencyRateAction => 30.0,
            RegimeEvent::GeopoliticalEscalation => 20.0,
            RegimeEvent::TradePolicyShift => 15.0,
            RegimeEvent::FinancialCrisis => 35.0,
            RegimeEvent::InflationShock => 20.0,
            RegimeEvent::None => 0.0,
        }
    }
    
    /// Suggested risk multiplier reduction when event detected
    pub fn risk_reduction(&self) -> f64 {
        match self {
            RegimeEvent::FedChairChange => 0.7,
            RegimeEvent::EmergencyRateAction => 0.5,
            RegimeEvent::GeopoliticalEscalation => 0.6,
            RegimeEvent::TradePolicyShift => 0.8,
            RegimeEvent::FinancialCrisis => 0.4,
            RegimeEvent::InflationShock => 0.7,
            RegimeEvent::None => 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewsApiClient {
    client: Client,
    api_key: String,
    cache: Arc<RwLock<Option<(NewsAnalysis, DateTime<Utc>)>>>,
}

/// Analyzed news output for MRATE consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsAnalysis {
    /// Overall market sentiment (-1.0 to 1.0)
    pub overall_sentiment: f64,
    /// Gold-specific sentiment (-1.0 to 1.0)
    pub gold_sentiment: f64,
    /// Fed/monetary policy sentiment (-1.0 to 1.0)  
    pub fed_sentiment: f64,
    /// Crypto sentiment (-1.0 to 1.0)
    pub crypto_sentiment: f64,
    /// Detected regime-shifting event (if any)
    pub regime_event: RegimeEvent,
    /// Description of detected regime event
    pub regime_event_headline: Option<String>,
    /// Number of articles analyzed
    pub articles_analyzed: usize,
    /// Bullish article ratio (0-1)
    pub bullish_ratio: f64,
    /// Bearish article ratio (0-1)
    pub bearish_ratio: f64,
    /// Headline keywords detected
    pub detected_keywords: Vec<String>,
    /// Timestamp of analysis
    pub timestamp: DateTime<Utc>,
}

/// Article for display in the news feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsApiArticle {
    pub title: String,
    pub description: String,
    pub url: String,
    pub source: String,
    pub published_at: String,
    pub sentiment_score: f64,
}

/// Raw NewsAPI response
#[derive(Debug, Deserialize)]
struct NewsApiResponse {
    status: String,
    #[serde(rename = "totalResults")]
    total_results: Option<i32>,
    articles: Option<Vec<RawArticle>>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawArticle {
    title: Option<String>,
    description: Option<String>,
    url: Option<String>,
    content: Option<String>,
    #[serde(rename = "publishedAt")]
    published_at: Option<String>,
    source: Option<RawSource>,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    name: Option<String>,
}

impl NewsApiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            cache: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Fetch and analyze news - main entry point for MRATE
    pub async fn get_analysis(&self) -> Option<NewsAnalysis> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some((cached, cached_at)) = cache.as_ref() {
                if (Utc::now() - *cached_at).num_seconds() < CACHE_DURATION_SECS {
                    return Some(cached.clone());
                }
            }
        }
        
        // Fetch fresh data
        match self.fetch_news().await {
            Ok(articles) => {
                let analysis = self.analyze_articles(&articles);
                
                // Update cache
                {
                    let mut cache = self.cache.write().await;
                    *cache = Some((analysis.clone(), Utc::now()));
                }
                
                info!(
                    "📰 News analysis: sentiment={:.2}, gold={:.2}, fed={:.2}, regime={:?}, articles={}",
                    analysis.overall_sentiment,
                    analysis.gold_sentiment,
                    analysis.fed_sentiment,
                    analysis.regime_event,
                    analysis.articles_analyzed
                );
                
                Some(analysis)
            }
            Err(e) => {
                warn!("NewsAPI fetch failed: {}", e);
                None
            }
        }
    }
    
    /// Get articles for display in the news feed (with sentiment scores)
    pub async fn get_articles(&self) -> Result<Vec<NewsApiArticle>, String> {
        let raw_articles = self.fetch_news().await?;
        
        let articles: Vec<NewsApiArticle> = raw_articles.into_iter()
            .filter_map(|a| {
                let title = a.title?;
                let description = a.description.unwrap_or_default();
                let text = format!("{} {}", title, description).to_lowercase();
                let sentiment_score = self.calculate_sentiment(&text);
                
                Some(NewsApiArticle {
                    title,
                    description,
                    url: a.url.unwrap_or_default(),
                    source: a.source.and_then(|s| s.name).unwrap_or_else(|| "Unknown".to_string()),
                    published_at: a.published_at.unwrap_or_default(),
                    sentiment_score,
                })
            })
            .collect();
        
        Ok(articles)
    }
    
    /// Fetch news articles from NewsAPI
    async fn fetch_news(&self) -> Result<Vec<RawArticle>, String> {
        // Query for financial/macro news relevant to trading
        let query = "federal reserve OR gold price OR inflation OR interest rates OR FOMC OR treasury yields";
        let url = format!(
            "{}/everything?q={}&language=en&sortBy=publishedAt&pageSize=50&apiKey={}",
            NEWSAPI_BASE_URL,
            urlencoding::encode(query),
            self.api_key
        );
        
        let response = self.client
            .get(&url)
            .header("User-Agent", "AureumBot/1.0 (Trading Platform)")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("NewsAPI request failed: {}", e))?;
        
        let data: NewsApiResponse = response
            .json()
            .await
            .map_err(|e| format!("NewsAPI parse failed: {}", e))?;
        
        if data.status != "ok" {
            return Err(format!(
                "NewsAPI error: {} - {}",
                data.code.unwrap_or_default(),
                data.message.unwrap_or_default()
            ));
        }
        
        Ok(data.articles.unwrap_or_default())
    }
    
    /// Analyze articles for sentiment and regime events
    fn analyze_articles(&self, articles: &[RawArticle]) -> NewsAnalysis {
        let mut overall_scores: Vec<f64> = Vec::new();
        let mut gold_scores: Vec<f64> = Vec::new();
        let mut fed_scores: Vec<f64> = Vec::new();
        let mut crypto_scores: Vec<f64> = Vec::new();
        let mut detected_keywords: HashSet<String> = HashSet::new();
        let mut regime_event = RegimeEvent::None;
        let mut regime_headline: Option<String> = None;
        
        for article in articles {
            let title = article.title.as_deref().unwrap_or("");
            let description = article.description.as_deref().unwrap_or("");
            let text = format!("{} {}", title, description).to_lowercase();
            
            // Calculate sentiment for this article
            let sentiment = self.calculate_sentiment(&text);
            overall_scores.push(sentiment);
            
            // Categorize by topic
            if self.is_gold_related(&text) {
                gold_scores.push(sentiment);
            }
            if self.is_fed_related(&text) {
                fed_scores.push(sentiment);
            }
            if self.is_crypto_related(&text) {
                crypto_scores.push(sentiment);
            }
            
            // Detect regime-shifting events (check title only for precision)
            let title_lower = title.to_lowercase();
            if let Some(event) = self.detect_regime_event(&title_lower) {
                if event != RegimeEvent::None {
                    regime_event = event;
                    regime_headline = Some(title.to_string());
                    info!("🚨 Regime event detected: {:?} - {}", regime_event, title);
                }
            }
            
            // Collect detected keywords
            for kw in self.extract_keywords(&text) {
                detected_keywords.insert(kw);
            }
        }
        
        // Calculate averages
        let overall_sentiment = if overall_scores.is_empty() {
            0.0
        } else {
            overall_scores.iter().sum::<f64>() / overall_scores.len() as f64
        };
        
        let gold_sentiment = if gold_scores.is_empty() {
            0.0
        } else {
            gold_scores.iter().sum::<f64>() / gold_scores.len() as f64
        };
        
        let fed_sentiment = if fed_scores.is_empty() {
            0.0
        } else {
            fed_scores.iter().sum::<f64>() / fed_scores.len() as f64
        };
        
        let crypto_sentiment = if crypto_scores.is_empty() {
            0.0
        } else {
            crypto_scores.iter().sum::<f64>() / crypto_scores.len() as f64
        };
        
        // Calculate bullish/bearish ratios
        let bullish_count = overall_scores.iter().filter(|&&s| s > 0.1).count();
        let bearish_count = overall_scores.iter().filter(|&&s| s < -0.1).count();
        let total = overall_scores.len().max(1) as f64;
        
        NewsAnalysis {
            overall_sentiment,
            gold_sentiment,
            fed_sentiment,
            crypto_sentiment,
            regime_event,
            regime_event_headline: regime_headline,
            articles_analyzed: articles.len(),
            bullish_ratio: bullish_count as f64 / total,
            bearish_ratio: bearish_count as f64 / total,
            detected_keywords: detected_keywords.into_iter().collect(),
            timestamp: Utc::now(),
        }
    }
    
    /// Simple but effective keyword-based sentiment analysis
    fn calculate_sentiment(&self, text: &str) -> f64 {
        let bullish_words = [
            "surge", "rally", "gain", "rise", "jump", "soar", "climb", "boost",
            "bullish", "optimistic", "strong", "growth", "recover", "rebound",
            "up", "higher", "record high", "all-time high", "breakthrough",
            "dovish", "rate cut", "easing", "stimulus", "support", "buy",
        ];
        
        let bearish_words = [
            "crash", "plunge", "drop", "fall", "decline", "sink", "tumble", "slump",
            "bearish", "pessimistic", "weak", "recession", "collapse", "crisis",
            "down", "lower", "sell-off", "selloff", "correction", "fear",
            "hawkish", "rate hike", "tightening", "inflation", "sell",
        ];
        
        let mut score: f64 = 0.0;
        
        for word in &bullish_words {
            if text.contains(word) {
                score += 0.15;
            }
        }
        
        for word in &bearish_words {
            if text.contains(word) {
                score -= 0.15;
            }
        }
        
        // Clamp to -1.0 to 1.0
        score.clamp(-1.0, 1.0)
    }
    
    /// Detect regime-shifting events from headline
    fn detect_regime_event(&self, title: &str) -> Option<RegimeEvent> {
        // Fed Chair change - MAJOR regime shift
        if (title.contains("fed chair") || title.contains("federal reserve chair"))
            && (title.contains("appoint") || title.contains("nominate") || title.contains("name") || title.contains("pick"))
        {
            return Some(RegimeEvent::FedChairChange);
        }
        
        // Emergency rate action
        if title.contains("emergency") && (title.contains("rate") || title.contains("fed") || title.contains("fomc")) {
            return Some(RegimeEvent::EmergencyRateAction);
        }
        
        // Financial crisis signals
        if title.contains("bank") && (title.contains("collapse") || title.contains("fail") || title.contains("crisis") || title.contains("run")) {
            return Some(RegimeEvent::FinancialCrisis);
        }
        
        // Geopolitical escalation
        if (title.contains("war") || title.contains("invade") || title.contains("attack") || title.contains("sanction"))
            && (title.contains("russia") || title.contains("china") || title.contains("iran") || title.contains("israel"))
        {
            return Some(RegimeEvent::GeopoliticalEscalation);
        }
        
        // Trade policy shift
        if title.contains("tariff") && (title.contains("announce") || title.contains("impose") || title.contains("new") || title.contains("increase")) {
            return Some(RegimeEvent::TradePolicyShift);
        }
        
        // Inflation shock
        if title.contains("inflation") && (title.contains("surge") || title.contains("spike") || title.contains("shock") || title.contains("unexpected")) {
            return Some(RegimeEvent::InflationShock);
        }
        
        Some(RegimeEvent::None)
    }
    
    fn is_gold_related(&self, text: &str) -> bool {
        text.contains("gold") || text.contains("precious metal") || text.contains("bullion") || text.contains("xau")
    }
    
    fn is_fed_related(&self, text: &str) -> bool {
        text.contains("fed") || text.contains("fomc") || text.contains("powell") 
            || text.contains("interest rate") || text.contains("monetary policy")
            || text.contains("treasury") || text.contains("yield")
    }
    
    fn is_crypto_related(&self, text: &str) -> bool {
        text.contains("bitcoin") || text.contains("btc") || text.contains("crypto") || text.contains("ethereum")
    }
    
    fn extract_keywords(&self, text: &str) -> Vec<String> {
        let important_keywords = [
            "fed", "fomc", "rate cut", "rate hike", "inflation", "recession",
            "gold", "bitcoin", "treasury", "yield", "dollar", "tariff",
            "war", "crisis", "rally", "crash", "stimulus",
        ];
        
        important_keywords
            .iter()
            .filter(|&kw| text.contains(kw))
            .map(|&s| s.to_string())
            .collect()
    }
}
