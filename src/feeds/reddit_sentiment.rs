use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use std::collections::HashMap;

/// Reddit Sentiment Aggregator
/// Uses public Reddit API (no auth required for read-only)
/// Analyzes top posts from r/CryptoCurrency, r/Bitcoin, r/WallStreetBets

const USER_AGENT: &str = "AureumBot/1.0";

#[derive(Debug, Clone)]
pub struct RedditSentimentClient {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedditSentiment {
    /// Overall crypto sentiment (-1.0 to 1.0)
    pub crypto_sentiment: f64,
    /// Bitcoin-specific sentiment
    pub btc_sentiment: f64,
    /// Gold mentions sentiment (from wsb, investing, etc.)
    pub gold_sentiment: Option<f64>,
    /// Total posts analyzed
    pub posts_analyzed: usize,
    /// Sentiment dispersion (high = conflicting signals)
    pub dispersion: f64,
    /// Bullish/bearish consensus
    pub bullish_consensus: bool,
    pub bearish_consensus: bool,
}

#[derive(Debug, Deserialize)]
struct RedditResponse {
    data: RedditData,
}

#[derive(Debug, Deserialize)]
struct RedditData {
    children: Vec<RedditPost>,
}

#[derive(Debug, Deserialize)]
struct RedditPost {
    data: PostData,
}

#[derive(Debug, Deserialize)]
struct PostData {
    title: String,
    selftext: String,
    score: i64,
    num_comments: i64,
    #[serde(default)]
    upvote_ratio: f64,
}

impl RedditSentimentClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
        }
    }
    
    /// Fetch and analyze sentiment from crypto subreddits
    pub async fn get_sentiment(&self) -> Option<RedditSentiment> {
        let mut all_scores = Vec::new();
        let mut btc_scores = Vec::new();
        let mut gold_scores = Vec::new();
        
        // Fetch from r/CryptoCurrency (hot posts)
        if let Some(posts) = self.fetch_subreddit_posts("CryptoCurrency", 25).await {
            for post in posts {
                let sentiment = self.analyze_post_sentiment(&post);
                all_scores.push(sentiment);
                
                if self.mentions_btc(&post) {
                    btc_scores.push(sentiment);
                }
                if self.mentions_gold(&post) {
                    gold_scores.push(sentiment);
                }
            }
        }
        
        // Fetch from r/Bitcoin
        if let Some(posts) = self.fetch_subreddit_posts("Bitcoin", 15).await {
            for post in posts {
                let sentiment = self.analyze_post_sentiment(&post);
                all_scores.push(sentiment);
                btc_scores.push(sentiment);
            }
        }
        
        // Fetch from r/WallStreetBets (for gold mentions)
        if let Some(posts) = self.fetch_subreddit_posts("wallstreetbets", 10).await {
            for post in posts {
                if self.mentions_gold(&post) {
                    let sentiment = self.analyze_post_sentiment(&post);
                    gold_scores.push(sentiment);
                    all_scores.push(sentiment);
                }
            }
        }
        
        if all_scores.is_empty() {
            warn!("No Reddit posts retrieved");
            return None;
        }
        
        // Calculate aggregate metrics
        let crypto_sentiment = all_scores.iter().sum::<f64>() / all_scores.len() as f64;
        let btc_sentiment = if !btc_scores.is_empty() {
            btc_scores.iter().sum::<f64>() / btc_scores.len() as f64
        } else {
            crypto_sentiment
        };
        let gold_sentiment = if !gold_scores.is_empty() {
            Some(gold_scores.iter().sum::<f64>() / gold_scores.len() as f64)
        } else {
            None
        };
        
        // Dispersion (std deviation)
        let mean = crypto_sentiment;
        let variance = all_scores.iter()
            .map(|s| (s - mean).powi(2))
            .sum::<f64>() / all_scores.len() as f64;
        let dispersion = variance.sqrt();
        
        // Consensus detection
        let bullish_count = all_scores.iter().filter(|&&s| s > 0.2).count();
        let bearish_count = all_scores.iter().filter(|&&s| s < -0.2).count();
        let bullish_consensus = bullish_count as f64 / all_scores.len() as f64 > 0.6;
        let bearish_consensus = bearish_count as f64 / all_scores.len() as f64 > 0.6;
        
        info!(
            "Reddit sentiment: crypto={:.3}, btc={:.3}, gold={:?}, posts={}, dispersion={:.3}",
            crypto_sentiment, btc_sentiment, gold_sentiment, all_scores.len(), dispersion
        );
        
        Some(RedditSentiment {
            crypto_sentiment,
            btc_sentiment,
            gold_sentiment,
            posts_analyzed: all_scores.len(),
            dispersion,
            bullish_consensus,
            bearish_consensus,
        })
    }
    
    /// Fetch hot posts from a subreddit
    async fn fetch_subreddit_posts(&self, subreddit: &str, limit: usize) -> Option<Vec<PostData>> {
        let url = format!("https://www.reddit.com/r/{}/hot.json?limit={}", subreddit, limit);
        
        let response = self.client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .ok()?;
        
        if !response.status().is_success() {
            warn!("Reddit API returned {} for r/{}", response.status(), subreddit);
            return None;
        }
        
        let reddit_response: RedditResponse = response.json().await.ok()?;
        
        Some(reddit_response.data.children.into_iter()
            .map(|post| post.data)
            .collect())
    }
    
    /// Analyze sentiment of a single post
    /// Returns -1.0 (bearish) to 1.0 (bullish)
    fn analyze_post_sentiment(&self, post: &PostData) -> f64 {
        // Use upvote ratio as primary signal
        // Reddit's upvote_ratio: 0.5 = neutral, >0.5 = positive engagement
        let engagement_sentiment = (post.upvote_ratio - 0.5) * 2.0;
        
        // Keyword-based sentiment (simple approach)
        let text = format!("{} {}", post.title.to_lowercase(), post.selftext.to_lowercase());
        let keyword_sentiment = self.keyword_sentiment(&text);
        
        // Weight: 70% keywords, 30% engagement
        (keyword_sentiment * 0.7 + engagement_sentiment * 0.3).clamp(-1.0, 1.0)
    }
    
    /// Simple keyword-based sentiment
    fn keyword_sentiment(&self, text: &str) -> f64 {
        let bullish_keywords = [
            "bullish", "moon", "pump", "rally", "breakout", "surge", "gains",
            "buy", "long", "calls", "rocket", "🚀", "ath", "parabolic", "adoption",
        ];
        let bearish_keywords = [
            "bearish", "dump", "crash", "selloff", "drop", "bear", "short",
            "puts", "collapse", "rekt", "scam", "fail", "fraud", "panic",
        ];
        
        let mut bullish_count = 0;
        let mut bearish_count = 0;
        
        for keyword in &bullish_keywords {
            bullish_count += text.matches(keyword).count();
        }
        for keyword in &bearish_keywords {
            bearish_count += text.matches(keyword).count();
        }
        
        if bullish_count == 0 && bearish_count == 0 {
            return 0.0;
        }
        
        let total = (bullish_count + bearish_count) as f64;
        (bullish_count as f64 - bearish_count as f64) / total
    }
    
    /// Check if post mentions BTC/Bitcoin
    fn mentions_btc(&self, post: &PostData) -> bool {
        let text = format!("{} {}", post.title.to_lowercase(), post.selftext.to_lowercase());
        text.contains("bitcoin") || text.contains("btc") || text.contains(" $btc")
    }
    
    /// Check if post mentions gold/XAU
    fn mentions_gold(&self, post: &PostData) -> bool {
        let text = format!("{} {}", post.title.to_lowercase(), post.selftext.to_lowercase());
        text.contains("gold") || text.contains("gld") || text.contains("xau") || text.contains("$gld")
    }
}

impl Default for RedditSentimentClient {
    fn default() -> Self {
        Self::new()
    }
}
