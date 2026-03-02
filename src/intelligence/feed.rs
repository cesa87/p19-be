//! Intelligence Feed Ingestion
//!
//! Sources:
//!   1. Telegram public channels (t.me/s/ scraping) — classified by geo-tagged feed_source
//!   2. NewsAPI top-headlines per country — gives global geographic coverage
//!
//! Every post gets a lat/lng (inherited from source + small deterministic jitter).
//! This drives the world-map node placement on the Intelligence Terminal.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use super::rss_ingest;

// ─── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedSource {
    pub id: Uuid,
    pub source_type: String,
    pub name: String,
    pub handle: String,
    pub enabled: bool,
    pub default_severity: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub country_code: Option<String>,
    pub category: String,
    pub reliability: f64,
    pub tier: i32,
    pub feed_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedPost {
    pub id: Uuid,
    pub source_id: Uuid,
    pub external_id: String,
    pub author: String,
    pub content: String,
    pub image_url: Option<String>,
    pub severity: String,
    pub severity_reason: Option<String>,
    pub related_markets: Vec<String>,
    pub source_url: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub posted_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedPostResponse {
    pub id: Uuid,
    pub source_id: Uuid,
    pub source_name: String,
    pub source_handle: String,
    pub source_type: String,
    pub source_category: String,
    pub source_reliability: f64,
    pub source_tier: i32,
    pub external_id: String,
    pub author: String,
    pub content: String,
    pub image_url: Option<String>,
    pub severity: String,
    pub severity_reason: Option<String>,
    pub related_markets: Vec<String>,
    pub source_url: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub posted_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

// ─── Geography helpers ────────────────────────────────────────────────────────

/// Map ISO-3166-1 alpha-2 country codes to (lat, lng) centroid.
fn country_coords(cc: &str) -> Option<(f64, f64)> {
    Some(match cc.to_lowercase().as_str() {
        "us" => (37.0,  -95.0),
        "gb" => (51.5,   -0.1),
        "de" => (51.0,   10.0),
        "fr" => (46.0,    2.0),
        "in" => (20.0,   78.0),
        "jp" => (36.0,  138.0),
        "au" => (-25.0, 135.0),
        "ca" => (56.0,  -96.0),
        "il" => (31.0,   35.0),
        "ps" => (31.9,   35.2),
        "eg" => (27.0,   30.0),
        "sa" => (24.0,   45.0),
        "ru" => (61.0,  105.0),
        "cn" => (35.0,  104.0),
        "br" => (-14.0, -51.0),
        "za" => (-30.0,  25.0),
        "ua" => (48.0,   32.0),
        "by" => (53.7,   28.0),
        "tr" => (39.0,   35.0),
        "ng" => (10.0,    8.0),
        "mx" => (23.0, -102.0),
        "kr" => (37.0,  127.5),
        "sg" => (1.3,   103.8),
        "hk" => (22.3,  114.1),
        "ke" => (-1.3,   36.8),
        "cl" => (-33.4, -70.6),
        "ar" => (-34.0, -64.0),
        "pk" => (30.0,   70.0),
        "ir" => (32.0,   53.0),
        _ => return None,
    })
}

/// Deterministic jitter based on the post's external_id string.
/// Produces consistent offsets so the same post always renders at the same spot.
fn coord_jitter(seed: &str, range: f64) -> (f64, f64) {
    let mut h: u64 = 5381;
    for b in seed.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    let jx = ((h % 1000) as f64 / 1000.0 - 0.5) * range * 2.0;
    let jy = (((h >> 16) % 1000) as f64 / 1000.0 - 0.5) * range * 2.0;
    (jx, jy)
}

// ─── OpenAI severity scorer ───────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"
You are a financial market intelligence analyst. Given a social media / Telegram post,
assess its market relevance and severity. Respond with ONLY a JSON object:
{
  "severity": "LOW|MEDIUM|HIGH|CRITICAL",
  "reason": "one concise sentence explaining why",
  "related_markets": ["market slug 1", "market slug 2"]
}

Severity guidelines:
- CRITICAL: imminent major market event (Fed rate decision, war declaration, exchange collapse)
- HIGH: significant macro signal (CPI beat/miss, major geopolitical event, large central bank action)
- MEDIUM: notable but not immediate (earnings beat, analyst upgrade, geopolitical tension)
- LOW: general commentary, noise, opinion with no clear market catalyst
"#;

pub async fn score_severity_openai(
    http: &Client,
    api_key: &str,
    content: &str,
) -> Option<(String, String, Vec<String>)> {
    #[derive(Serialize)]
    struct Req { model: &'static str, messages: Vec<Msg>, max_tokens: u32, temperature: f32 }
    #[derive(Serialize)]
    struct Msg { role: &'static str, content: String }
    #[derive(Deserialize)]
    struct Resp { choices: Vec<Choice> }
    #[derive(Deserialize)]
    struct Choice { message: MsgResp }
    #[derive(Deserialize)]
    struct MsgResp { content: String }
    #[derive(Deserialize)]
    struct Scored { severity: String, reason: String, #[serde(default)] related_markets: Vec<String> }

    let trimmed = if content.len() > 800 { &content[..800] } else { content };
    let req = Req {
        model: "gpt-4o-mini",
        messages: vec![
            Msg { role: "system", content: SYSTEM_PROMPT.to_string() },
            Msg { role: "user", content: trimmed.to_string() },
        ],
        max_tokens: 120,
        temperature: 0.2,
    };

    let resp = http.post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key).json(&req).send().await.ok()?;

    if !resp.status().is_success() {
        warn!("OpenAI severity score failed: {}", resp.status());
        return None;
    }

    let body: Resp = resp.json().await.ok()?;
    let text = body.choices.first()?.message.content.trim().to_string();
    let json_str = text.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let scored: Scored = serde_json::from_str(json_str).ok()?;
    Some((scored.severity, scored.reason, scored.related_markets))
}

// ─── Telegram t.me/s/ scraper ─────────────────────────────────────────────────

#[derive(Debug)]
struct RawPost {
    external_id: String,
    author: String,
    content: String,
    image_url: Option<String>,
    source_url: Option<String>,
    posted_at: DateTime<Utc>,
}

async fn fetch_telegram_channel_posts(
    http: &Client,
    handle: &str,
    limit: i64,
) -> Vec<RawPost> {
    let url = format!("https://t.me/s/{}", handle.trim_start_matches('@'));
    let raw_html = match http.get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send().await
    {
        Ok(r) if r.status().is_success() => match r.text().await { Ok(t) => t, Err(_) => return vec![] },
        _ => return vec![],
    };
    parse_tme_html(&raw_html, handle, limit)
}

fn parse_tme_html(html: &str, handle: &str, limit: i64) -> Vec<RawPost> {
    let mut posts = Vec::new();
    let channel = handle.trim_start_matches('@');
    for part in html.split("tgme_widget_message_wrap") {
        if part.contains("data-post=") && posts.len() < limit as usize {
            let ext_id = extract_between(part, "data-post=\"", "\"")
                .map(|s| s.split('/').last().unwrap_or("0").to_string())
                .unwrap_or_else(|| "0".to_string());
            if ext_id == "0" { continue; }
            let content = extract_message_text(part);
            if content.trim().is_empty() { continue; }
            let image_url = extract_between(part, "background-image:url('", "')")
                .map(|s| s.to_string());
            let posted_at = extract_between(part, "datetime=\"", "\"")
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            posts.push(RawPost {
                external_id: format!("{}:{}", channel, ext_id),
                author: channel.to_string(),
                content: content.trim().to_string(),
                image_url,
                source_url: Some(format!("https://t.me/{}/{}", channel, ext_id)),
                posted_at,
            });
        }
    }
    posts.reverse();
    posts
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

fn extract_message_text(html_block: &str) -> String {
    let block = match html_block.find("tgme_widget_message_text") {
        Some(i) => &html_block[i..],
        None => return String::new(),
    };
    let raw = &block[block.find('>').map(|i| i + 1).unwrap_or(0)..];
    let mut result = String::new();
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
        if result.len() > 1500 { break; }
    }
    result.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
          .replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
          .trim().to_string()
}

// ─── NewsAPI multi-country ingestion ─────────────────────────────────────────

/// Countries to pull from NewsAPI — ordered by geopolitical importance for the map.
const NEWSAPI_COUNTRIES: &[&str] = &[
    "us", "gb", "de", "fr", "in", "jp", "il", "ua", "sa", "cn", "br", "za", "ru", "au", "ca",
];

#[derive(Deserialize)]
struct NewsApiResponse {
    articles: Option<Vec<NewsApiArticle>>,
}

#[derive(Deserialize)]
struct NewsApiArticle {
    source: NewsApiSource,
    title: Option<String>,
    description: Option<String>,
    url: Option<String>,
    #[serde(rename = "urlToImage")]
    url_to_image: Option<String>,
    #[serde(rename = "publishedAt")]
    published_at: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
struct NewsApiSource {
    name: Option<String>,
    id: Option<String>,
}

async fn ingest_newsapi(pool: &PgPool, http: &Client, api_key: &str, openai_key: Option<&str>) {
    for &cc in NEWSAPI_COUNTRIES {
        // Ensure a feed_source exists for this country's news
        let source_handle = format!("newsapi_{}", cc);
        let (lat, lng) = match country_coords(cc) {
            Some(c) => c,
            None => continue,
        };

        let source: FeedSource = match sqlx::query_as::<_, FeedSource>(r#"
            INSERT INTO feed_sources (source_type, name, handle, default_severity, latitude, longitude, country_code)
            VALUES ('newsapi', $1, $2, 'MEDIUM', $3, $4, $5)
            ON CONFLICT (source_type, handle) DO UPDATE
                SET latitude = EXCLUDED.latitude, longitude = EXCLUDED.longitude
            RETURNING *
        "#)
        .bind(format!("NewsAPI {}", cc.to_uppercase()))
        .bind(&source_handle)
        .bind(lat)
        .bind(lng)
        .bind(cc.to_uppercase())
        .fetch_one(pool)
        .await {
            Ok(s) => s,
            Err(e) => { warn!("NewsAPI source upsert failed for {}: {}", cc, e); continue; }
        };

        let url = format!(
            "https://newsapi.org/v2/top-headlines?country={}&pageSize=10&apiKey={}",
            cc, api_key
        );

        let articles = match http.get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send().await
        {
            Ok(r) if r.status().is_success() => {
                match r.json::<NewsApiResponse>().await {
                    Ok(b) => b.articles.unwrap_or_default(),
                    Err(e) => { warn!("NewsAPI parse error for {}: {}", cc, e); continue; }
                }
            }
            Ok(r) => { warn!("NewsAPI {} returned {}", cc, r.status()); continue; }
            Err(e) => { warn!("NewsAPI request failed for {}: {}", cc, e); continue; }
        };

        let mut new_count = 0;
        for article in &articles {
            let title = match &article.title { Some(t) if !t.is_empty() => t, _ => continue };
            let content = article.description.as_deref()
                .or(article.content.as_deref())
                .unwrap_or(title.as_str())
                .to_string();
            let ext_id = format!("newsapi:{}:{}", cc, {
                let mut h: u64 = 5381;
                for b in title.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
                h
            });

            let published_at = article.published_at.as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            let severity = rss_ingest::quick_severity(&content).to_string();
            let reason   = rss_ingest::severity_reason(&severity, &content);
            let markets: Vec<String> = vec![];

            // Apply small jitter so articles from same country spread slightly
            let (jx, jy) = coord_jitter(&ext_id, 2.5);
            let post_lat = lat + jy;
            let post_lng = lng + jx;

            let _ = sqlx::query(r#"
                INSERT INTO feed_posts
                    (source_id, external_id, author, content, image_url, severity,
                     severity_reason, related_markets, source_url, posted_at, latitude, longitude)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                ON CONFLICT (source_id, external_id) DO NOTHING
            "#)
            .bind(source.id)
            .bind(&ext_id)
            .bind(article.source.name.as_deref().unwrap_or("NewsAPI"))
            .bind(&content)
            .bind(&article.url_to_image)
            .bind(&severity)
            .bind(&reason)
            .bind(&markets)
            .bind(&article.url)
            .bind(published_at)
            .bind(post_lat)
            .bind(post_lng)
            .execute(pool)
            .await;

            new_count += 1;
        }

        if new_count > 0 {
            info!("📰 NewsAPI {}: {} articles ingested", cc.to_uppercase(), new_count);
        }
    }
}

// ─── Main ingester ────────────────────────────────────────────────────────────

pub struct FeedIngester {
    pool: PgPool,
    http: Client,
    openai_key: Option<String>,
    bot_token: Option<String>,
    newsapi_key: Option<String>,
}

impl FeedIngester {
    pub fn new(pool: PgPool, config: &Config) -> Self {
        Self {
            pool,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .user_agent("Mozilla/5.0 (compatible; AureumIntel/1.0)")
                .build()
                .unwrap_or_default(),
            openai_key: config.openai_api_key.clone(),
            bot_token: config.telegram_bot_token.clone(),
            newsapi_key: config.newsapi_key.clone(),
        }
    }

    pub async fn ingest_all(&self) {
        // RSS/Atom + CryptoPanic — zero API cost, fast keyword scoring
        rss_ingest::ingest_rss_sources(&self.pool, &self.http).await;
        rss_ingest::ingest_cryptopanic(&self.pool, &self.http).await;

        let sources: Vec<FeedSource> = match sqlx::query_as::<_, FeedSource>(
            "SELECT * FROM feed_sources WHERE enabled = true AND source_type = 'telegram' ORDER BY name"
        ).fetch_all(&self.pool).await {
            Ok(s) => s,
            Err(e) => { error!("Failed to load feed sources: {}", e); return; }
        };

        if !sources.is_empty() {
            info!("🔄 Telegram feed ingestion: {} sources", sources.len());
            for source in &sources {
                if let Err(e) = self.ingest_telegram_source(source).await {
                    warn!("Feed ingest failed for {}: {}", source.handle, e);
                }
            }
        }

        // NewsAPI — global geographic coverage
        if let Some(key) = &self.newsapi_key {
            ingest_newsapi(&self.pool, &self.http, key, self.openai_key.as_deref()).await;
        }
    }

    async fn ingest_telegram_source(&self, source: &FeedSource) -> Result<(), String> {
        let last_id: Option<String> = sqlx::query_scalar(
            "SELECT external_id FROM feed_posts WHERE source_id = $1 ORDER BY posted_at DESC LIMIT 1"
        )
        .bind(source.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let posts = fetch_telegram_channel_posts(&self.http, &source.handle, 10).await;
        let new_posts: Vec<_> = posts.into_iter()
            .filter(|p| last_id.as_deref().map(|l| p.external_id != l).unwrap_or(true))
            .collect();

        if new_posts.is_empty() { return Ok(()); }
        info!("📨 {} new posts from @{}", new_posts.len(), source.handle);

        for post in new_posts {
            let severity = rss_ingest::quick_severity(&post.content).to_string();
            let reason   = rss_ingest::severity_reason(&severity, &post.content);
            let markets: Vec<String> = vec![];

            // Geographic coordinates from source + deterministic jitter
            let (post_lat, post_lng) = match (source.latitude, source.longitude) {
                (Some(lat), Some(lng)) => {
                    let (jx, jy) = coord_jitter(&post.external_id, 1.5);
                    (Some(lat + jy), Some(lng + jx))
                }
                _ => (None, None),
            };

            let _ = sqlx::query(r#"
                INSERT INTO feed_posts
                    (source_id, external_id, author, content, image_url, severity,
                     severity_reason, related_markets, source_url, posted_at, latitude, longitude)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                ON CONFLICT (source_id, external_id) DO NOTHING
            "#)
            .bind(source.id).bind(&post.external_id).bind(&post.author)
            .bind(&post.content).bind(&post.image_url).bind(&severity)
            .bind(&reason).bind(&markets).bind(&post.source_url)
            .bind(post.posted_at).bind(post_lat).bind(post_lng)
            .execute(&self.pool).await;
        }
        Ok(())
    }
}

// ─── DB helpers for API ───────────────────────────────────────────────────────

pub async fn get_feed_posts(
    pool: &PgPool,
    limit: i64,
    source_handle: Option<&str>,
    severity_filter: Option<&str>,
    category_filter: Option<&str>,
) -> Result<Vec<FeedPostResponse>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT fp.id, fp.source_id, fs.name AS source_name, fs.handle AS source_handle,
               fs.source_type, fs.category AS source_category,
               fs.reliability AS source_reliability, fs.tier AS source_tier,
               fp.external_id, fp.author, fp.content, fp.image_url,
               fp.severity, fp.severity_reason, fp.related_markets, fp.source_url,
               fp.latitude, fp.longitude, fp.posted_at, fp.ingested_at
        FROM feed_posts fp
        JOIN feed_sources fs ON fs.id = fp.source_id
        WHERE ($1::text IS NULL OR fs.handle = $1)
          AND ($2::text IS NULL OR fp.severity = $2)
          AND ($3::text IS NULL OR fs.category = $3)
        ORDER BY fp.posted_at DESC
        LIMIT $4
        "#,
        source_handle, severity_filter, category_filter, limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| FeedPostResponse {
        id: r.id,
        source_id: r.source_id,
        source_name: r.source_name,
        source_handle: r.source_handle,
        source_type: r.source_type,
        source_category: r.source_category,
        source_reliability: r.source_reliability,
        source_tier: r.source_tier,
        external_id: r.external_id,
        author: r.author,
        content: r.content,
        image_url: r.image_url,
        severity: r.severity,
        severity_reason: r.severity_reason,
        related_markets: r.related_markets,
        source_url: r.source_url,
        latitude: r.latitude,
        longitude: r.longitude,
        posted_at: r.posted_at,
        ingested_at: r.ingested_at,
    }).collect())
}

pub async fn get_feed_sources(pool: &PgPool) -> Result<Vec<FeedSource>, sqlx::Error> {
    sqlx::query_as::<_, FeedSource>("SELECT * FROM feed_sources ORDER BY name")
        .fetch_all(pool).await
}

pub async fn add_feed_source(
    pool: &PgPool, source_type: &str, name: &str, handle: &str, default_severity: &str,
) -> Result<FeedSource, sqlx::Error> {
    sqlx::query_as::<_, FeedSource>(r#"
        INSERT INTO feed_sources (source_type, name, handle, default_severity)
        VALUES ($1,$2,$3,$4)
        ON CONFLICT (source_type, handle) DO UPDATE SET name = EXCLUDED.name, updated_at = NOW()
        RETURNING *
    "#)
    .bind(source_type).bind(name).bind(handle.trim_start_matches('@')).bind(default_severity)
    .fetch_one(pool).await
}

pub async fn delete_feed_source(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM feed_sources WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn toggle_feed_source(pool: &PgPool, id: Uuid, enabled: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE feed_sources SET enabled = $1, updated_at = NOW() WHERE id = $2")
        .bind(enabled).bind(id).execute(pool).await?;
    Ok(())
}
