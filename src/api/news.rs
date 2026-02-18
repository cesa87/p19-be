use actix_web::{web, HttpResponse, Responder};
use chrono::{Duration, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::error;

use crate::config::Config;
use crate::news::{EconomicEvent, FinnhubClient, MultiSourceNews, UnifiedNewsArticle, NewsSentiment};

/// Cache for API responses
struct NewsCache {
    calendar: Option<(Vec<EconomicEvent>, chrono::DateTime<Utc>)>,
    news: Option<(Vec<UnifiedNewsArticle>, Option<NewsSentiment>, chrono::DateTime<Utc>)>,
}

impl NewsCache {
    fn new() -> Self {
        Self {
            calendar: None,
            news: None,
        }
    }
}

lazy_static::lazy_static! {
    static ref CACHE: Arc<RwLock<NewsCache>> = Arc::new(RwLock::new(NewsCache::new()));
}

const CACHE_DURATION_MINUTES: i64 = 5;

#[derive(Serialize)]
struct CalendarResponse {
    events: Vec<EconomicEvent>,
    cached: bool,
}

#[derive(Serialize)]
struct NewsFeedResponse {
    articles: Vec<UnifiedNewsArticle>,
    sentiment: Option<NewsSentiment>,
    cached: bool,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// GET /api/news/calendar - Get upcoming economic events
pub async fn get_economic_calendar(config: web::Data<Config>) -> impl Responder {
    let api_key = match &config.finnhub_api_key {
        Some(key) if !key.is_empty() => key.clone(),
        _ => {
            return HttpResponse::ServiceUnavailable().json(ErrorResponse {
                error: "Finnhub API key not configured".to_string(),
            });
        }
    };

    // Check cache first
    {
        let cache = CACHE.read().await;
        if let Some((events, cached_at)) = &cache.calendar {
            if Utc::now() - *cached_at < Duration::minutes(CACHE_DURATION_MINUTES) {
                return HttpResponse::Ok().json(CalendarResponse {
                    events: events.clone(),
                    cached: true,
                });
            }
        }
    }

    // Fetch fresh data
    let client = FinnhubClient::new(api_key);
    let today = Utc::now().date_naive();
    let next_week = today + Duration::days(7);

    match client.get_economic_calendar(today, next_week).await {
        Ok(events) => {
            // Update cache
            {
                let mut cache = CACHE.write().await;
                cache.calendar = Some((events.clone(), Utc::now()));
            }

            HttpResponse::Ok().json(CalendarResponse {
                events,
                cached: false,
            })
        }
        Err(e) => {
            error!("Failed to fetch economic calendar: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e })
        }
    }
}

/// GET /api/news/feed - Get market news with sentiment
/// Uses Alpha Vantage (primary, with sentiment) → Finnhub (fallback)
pub async fn get_news_feed(config: web::Data<Config>) -> impl Responder {
    // Check cache first
    {
        let cache = CACHE.read().await;
        if let Some((articles, sentiment, cached_at)) = &cache.news {
            if Utc::now() - *cached_at < Duration::minutes(CACHE_DURATION_MINUTES) {
                return HttpResponse::Ok().json(NewsFeedResponse {
                    articles: articles.clone(),
                    sentiment: sentiment.clone(),
                    cached: true,
                });
            }
        }
    }

    // Fetch fresh data from best available source
    let client = MultiSourceNews::new(
        config.newsapi_key.clone(),
        config.alpha_vantage_api_key.clone(),
        config.finnhub_api_key.clone(),
    );

    match client.get_news().await {
        Ok((articles, sentiment)) => {
            // Update cache
            {
                let mut cache = CACHE.write().await;
                cache.news = Some((articles.clone(), sentiment.clone(), Utc::now()));
            }

            HttpResponse::Ok().json(NewsFeedResponse {
                articles,
                sentiment,
                cached: false,
            })
        }
        Err(e) => {
            error!("Failed to fetch news feed: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse { error: e })
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/news")
            .route("/calendar", web::get().to(get_economic_calendar))
            .route("/feed", web::get().to(get_news_feed)),
    );
}
