use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

const FINNHUB_BASE_URL: &str = "https://finnhub.io/api/v1";

#[derive(Debug, Clone)]
pub struct FinnhubClient {
    client: Client,
    api_key: String,
}

/// Economic calendar event from Finnhub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicEvent {
    pub country: String,
    pub event: String,
    #[serde(rename = "time")]
    pub event_time: String,
    pub impact: String,  // "low", "medium", "high"
    #[serde(rename = "prev")]
    pub previous: Option<String>,
    #[serde(rename = "est")]
    pub estimate: Option<String>,
    pub actual: Option<String>,
    pub unit: Option<String>,
}

/// Raw response from Finnhub economic calendar
#[derive(Debug, Deserialize)]
struct EconomicCalendarResponse {
    #[serde(rename = "economicCalendar")]
    economic_calendar: Vec<RawEconomicEvent>,
}

#[derive(Debug, Deserialize)]
struct RawEconomicEvent {
    country: Option<String>,
    event: Option<String>,
    time: Option<String>,
    impact: Option<String>,
    prev: Option<f64>,
    est: Option<f64>,
    actual: Option<f64>,
    unit: Option<String>,
}

/// News article from Finnhub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsArticle {
    pub id: i64,
    pub headline: String,
    pub summary: String,
    pub source: String,
    pub url: String,
    pub image: Option<String>,
    #[serde(rename = "datetime")]
    pub timestamp: i64,  // Unix timestamp
    pub category: String,
    pub related: Option<String>,
}

/// Raw news response from Finnhub
#[derive(Debug, Deserialize)]
struct RawNewsArticle {
    id: Option<i64>,
    headline: Option<String>,
    summary: Option<String>,
    source: Option<String>,
    url: Option<String>,
    image: Option<String>,
    datetime: Option<i64>,
    category: Option<String>,
    related: Option<String>,
}

impl FinnhubClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Get economic calendar events for a date range
    pub async fn get_economic_calendar(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<EconomicEvent>, String> {
        let url = format!(
            "{}/calendar/economic?from={}&to={}&token={}",
            FINNHUB_BASE_URL,
            from.format("%Y-%m-%d"),
            to.format("%Y-%m-%d"),
            self.api_key
        );

        info!("Fetching economic calendar from Finnhub");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch economic calendar: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Finnhub economic calendar error: {} - {}", status, text);
            return Err(format!("Finnhub API error: {}", status));
        }

        let data: EconomicCalendarResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse economic calendar: {}", e))?;

        // Convert and filter for USD-relevant events
        let events: Vec<EconomicEvent> = data
            .economic_calendar
            .into_iter()
            .filter_map(|raw| {
                let country = raw.country?;
                // Focus on US events (most impactful for gold)
                if country != "US" {
                    return None;
                }
                
                Some(EconomicEvent {
                    country,
                    event: raw.event.unwrap_or_default(),
                    event_time: raw.time.unwrap_or_default(),
                    impact: raw.impact.unwrap_or_else(|| "low".to_string()),
                    previous: raw.prev.map(|v| format!("{:.2}", v)),
                    estimate: raw.est.map(|v| format!("{:.2}", v)),
                    actual: raw.actual.map(|v| format!("{:.2}", v)),
                    unit: raw.unit,
                })
            })
            .collect();

        // Sort by impact (high first) then by time
        let mut sorted_events = events;
        sorted_events.sort_by(|a, b| {
            let impact_order = |impact: &str| match impact {
                "high" => 0,
                "medium" => 1,
                _ => 2,
            };
            impact_order(&a.impact)
                .cmp(&impact_order(&b.impact))
                .then(a.event_time.cmp(&b.event_time))
        });

        Ok(sorted_events)
    }

    /// Get market news (forex/commodities category covers gold)
    pub async fn get_market_news(&self, category: &str) -> Result<Vec<NewsArticle>, String> {
        let url = format!(
            "{}/news?category={}&token={}",
            FINNHUB_BASE_URL, category, self.api_key
        );

        info!("Fetching market news from Finnhub, category: {}", category);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch market news: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("Finnhub news error: {} - {}", status, text);
            return Err(format!("Finnhub API error: {}", status));
        }

        let data: Vec<RawNewsArticle> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse market news: {}", e))?;

        // Convert and filter for gold-relevant news
        let gold_keywords = [
            "gold", "xau", "precious metal", "bullion", 
            "fed", "federal reserve", "interest rate", "inflation",
            "dollar", "usd", "treasury", "fomc", "powell",
            "safe haven", "commodity", "commodities"
        ];

        let articles: Vec<NewsArticle> = data
            .into_iter()
            .filter_map(|raw| {
                let headline = raw.headline.unwrap_or_default();
                let summary = raw.summary.unwrap_or_default();
                
                // Check if article is gold-relevant
                let text_lower = format!("{} {}", headline, summary).to_lowercase();
                let is_relevant = gold_keywords.iter().any(|kw| text_lower.contains(kw));
                
                if !is_relevant {
                    return None;
                }

                Some(NewsArticle {
                    id: raw.id.unwrap_or(0),
                    headline,
                    summary,
                    source: raw.source.unwrap_or_default(),
                    url: raw.url.unwrap_or_default(),
                    image: raw.image,
                    timestamp: raw.datetime.unwrap_or(0),
                    category: raw.category.unwrap_or_default(),
                    related: raw.related,
                })
            })
            .collect();

        Ok(articles)
    }

    /// Get gold-relevant news from multiple categories
    pub async fn get_gold_news(&self) -> Result<Vec<NewsArticle>, String> {
        // Fetch from forex category (covers commodities and gold)
        let mut all_news = self.get_market_news("forex").await?;
        
        // Also try general category
        if let Ok(general) = self.get_market_news("general").await {
            all_news.extend(general);
        }

        // Deduplicate by ID
        all_news.sort_by_key(|a| a.id);
        all_news.dedup_by_key(|a| a.id);

        // Sort by timestamp (newest first)
        all_news.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Limit to 50 articles
        all_news.truncate(50);

        Ok(all_news)
    }
}
