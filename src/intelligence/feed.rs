//! Intelligence Feed Ingestion
//!
//! Pulls posts from configured Telegram channels, scores severity via OpenAI,
//! and stores to feed_posts for the Intelligence Terminal feed drawer.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
// use std::sync::Arc;
// use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;
// use crate::telegram::mtproto_client::MtprotoClient;

// ─── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedSource {
    pub id: Uuid,
    pub source_type: String,
    pub name: String,
    pub handle: String,
    pub enabled: bool,
    pub default_severity: String,
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
    pub external_id: String,
    pub author: String,
    pub content: String,
    pub image_url: Option<String>,
    pub severity: String,
    pub severity_reason: Option<String>,
    pub related_markets: Vec<String>,
    pub source_url: Option<String>,
    pub posted_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

// ─── OpenAI severity scorer ───────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = r#"
You are a financial market intelligence analyst. Given a social media / Telegram post,
assess its market relevance and severity. Respond with ONLY a JSON object:
{
  "severity": "LOW|MEDIUM|HIGH|CRITICAL",
  "reason": "one concise sentence explaining why",
  "related_markets": ["market slug 1", "market slug 2"]  // up to 3 Polymarket-style slugs if relevant, else []
}

Severity guidelines:
- CRITICAL: imminent major market event (Fed rate decision, war declaration, exchange collapse)
- HIGH: significant macro signal (CPI beat/miss, major geopolitical event, large central bank action)
- MEDIUM: notable but not immediate (earnings beat, analyst upgrade, geopolitical tension)
- LOW: general commentary, noise, opinion with no clear market catalyst
"#;

async fn score_severity_openai(
    http: &Client,
    api_key: &str,
    content: &str,
) -> Option<(String, String, Vec<String>)> {
    #[derive(Serialize)]
    struct Req {
        model: &'static str,
        messages: Vec<Msg>,
        max_tokens: u32,
        temperature: f32,
    }
    #[derive(Serialize)]
    struct Msg {
        role: &'static str,
        content: String,
    }
    #[derive(Deserialize)]
    struct Resp {
        choices: Vec<Choice>,
    }
    #[derive(Deserialize)]
    struct Choice {
        message: MsgResp,
    }
    #[derive(Deserialize)]
    struct MsgResp {
        content: String,
    }
    #[derive(Deserialize)]
    struct Scored {
        severity: String,
        reason: String,
        #[serde(default)]
        related_markets: Vec<String>,
    }

    // Trim content to avoid burning tokens
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

    let resp = http
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&req)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        warn!("OpenAI severity score failed: {}", resp.status());
        return None;
    }

    let body: Resp = resp.json().await.ok()?;
    let text = body.choices.first()?.message.content.trim().to_string();

    // Strip possible ```json fences
    let json_str = text
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let scored: Scored = serde_json::from_str(json_str).ok()?;
    Some((scored.severity, scored.reason, scored.related_markets))
}

// ─── Telegram public channel fetcher (Bot API + web preview fallback) ─────────

/// Fetch recent messages from a public Telegram channel via Bot API.
/// The bot must be added to the channel as an admin, OR the channel is public
/// (in which case we use getUpdates / forwardMessage workaround).
///
/// For public channels we use the t.me/s/ JSON endpoint as a fallback.
async fn fetch_telegram_channel_posts(
    http: &Client,
    bot_token: &str,
    handle: &str,
    limit: i64,
    since_id: Option<i64>,
) -> Vec<RawPost> {
    // Primary: Bot API getUpdates isn't suitable for channels the bot isn't in.
    // Instead use the Telegram public channel web-preview endpoint which returns
    // JSON messages for @public channels.
    let url = format!("https://t.me/s/{}", handle.trim_start_matches('@'));

    let result = http
        .get(&url)
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    let raw_html = match result {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(t) => t,
            Err(_) => return vec![],
        },
        _ => return vec![],
    };

    // Parse t.me/s HTML to extract message previews.
    parse_tme_html(&raw_html, handle, limit)
}

#[derive(Debug)]
struct RawPost {
    external_id: String,
    author: String,
    content: String,
    image_url: Option<String>,
    source_url: Option<String>,
    posted_at: DateTime<Utc>,
}

/// Parse messages from t.me/s/{channel} HTML page.
/// Extracts text content and approximate timestamps from the DOM structure.
fn parse_tme_html(html: &str, handle: &str, limit: i64) -> Vec<RawPost> {
    let mut posts = Vec::new();
    let channel = handle.trim_start_matches('@');

    // t.me/s pages embed message data inline. We extract the message blocks.
    // Each message has: <div class="tgme_widget_message" data-post="{channel}/{id}">
    for part in html.split("tgme_widget_message_wrap") {
        if part.contains("data-post=") && posts.len() < limit as usize {
            // Extract message ID
            let ext_id = extract_between(part, "data-post=\"", "\"")
                .map(|s| s.split('/').last().unwrap_or("0").to_string())
                .unwrap_or_else(|| "0".to_string());

            if ext_id == "0" { continue; }

            // Extract text content
            let content = extract_message_text(part);
            if content.trim().is_empty() { continue; }

            // Extract image if present
            let image_url = extract_between(part, "background-image:url('", "')")
                .or_else(|| extract_between(part, r#"src=""#, r#"""#).filter(|s| s.contains("cdn")))
                .map(|s| s.to_string());

            // Extract timestamp (datetime attr)
            let posted_at = extract_between(part, r#"datetime=""#, r#"""#)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            let source_url = Some(format!("https://t.me/{}/{}", channel, ext_id));

            posts.push(RawPost {
                external_id: format!("{}:{}", channel, ext_id),
                author: channel.to_string(),
                content: content.trim().to_string(),
                image_url,
                source_url,
                posted_at,
            });
        }
    }

    posts.reverse(); // oldest first
    posts
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

fn extract_message_text(html_block: &str) -> String {
    // Find the message text div and strip HTML tags.
    let text_start = html_block.find("tgme_widget_message_text");
    let block = match text_start {
        Some(i) => &html_block[i..],
        None => return String::new(),
    };
    // Find the inner content after the first >
    let content_start = block.find('>').map(|i| i + 1).unwrap_or(0);
    let raw = &block[content_start..];
    // Crude HTML strip
    let mut result = String::new();
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
        // Stop at the end of the message text container (~2000 chars)
        if result.len() > 1500 { break; }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

// ─── Main ingester ────────────────────────────────────────────────────────────

pub struct FeedIngester {
    pool: PgPool,
    http: Client,
    openai_key: Option<String>,
    bot_token: Option<String>,
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
        }
    }

    /// Ingest all enabled sources. Safe to run on a schedule.
    pub async fn ingest_all(&self) {
        let sources: Vec<FeedSource> = match sqlx::query_as::<_, FeedSource>(
            "SELECT * FROM feed_sources WHERE enabled = true ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await {
            Ok(s) => s,
            Err(e) => { error!("Failed to load feed sources: {}", e); return; }
        };

        if sources.is_empty() {
            return;
        }

        info!("🔄 Feed ingestion: {} sources", sources.len());

        for source in &sources {
            if let Err(e) = self.ingest_source(source).await {
                warn!("Feed ingest failed for {}: {}", source.handle, e);
            }
        }
    }

    async fn ingest_source(&self, source: &FeedSource) -> Result<(), String> {
        // Get last ingested external_id to avoid re-scoring old posts
        let last_id: Option<String> = sqlx::query_scalar(
            "SELECT external_id FROM feed_posts WHERE source_id = $1 ORDER BY posted_at DESC LIMIT 1"
        )
        .bind(source.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        let posts = match source.source_type.as_str() {
            "telegram" => {
                let token = self.bot_token.as_deref().unwrap_or("");
                fetch_telegram_channel_posts(&self.http, token, &source.handle, 10, None).await
            }
            _ => {
                warn!("Unsupported source type: {}", source.source_type);
                return Ok(());
            }
        };

        // Filter posts already seen
        let new_posts: Vec<_> = posts
            .into_iter()
            .filter(|p| last_id.as_deref().map(|l| p.external_id != l).unwrap_or(true))
            .collect();

        if new_posts.is_empty() {
            return Ok(());
        }

        info!("📨 {} new posts from @{}", new_posts.len(), source.handle);

        for post in new_posts {
            // Score via OpenAI, fallback to source default
            let (severity, reason, markets) = if let Some(key) = &self.openai_key {
                score_severity_openai(&self.http, key, &post.content)
                    .await
                    .unwrap_or_else(|| (source.default_severity.clone(), "Fallback scoring".to_string(), vec![]))
            } else {
                (source.default_severity.clone(), "No OpenAI key".to_string(), vec![])
            };

            // Upsert — ignore duplicates
            let _ = sqlx::query(r#"
                INSERT INTO feed_posts
                    (source_id, external_id, author, content, image_url, severity,
                     severity_reason, related_markets, source_url, posted_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (source_id, external_id) DO NOTHING
            "#)
            .bind(source.id)
            .bind(&post.external_id)
            .bind(&post.author)
            .bind(&post.content)
            .bind(&post.image_url)
            .bind(&severity)
            .bind(&reason)
            .bind(&markets)
            .bind(&post.source_url)
            .bind(post.posted_at)
            .execute(&self.pool)
            .await;
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
) -> Result<Vec<FeedPostResponse>, sqlx::Error> {
    sqlx::query_as!(
        FeedPostResponse,
        r#"
        SELECT
            fp.id,
            fp.source_id,
            fs.name  AS source_name,
            fs.handle AS source_handle,
            fs.source_type,
            fp.external_id,
            fp.author,
            fp.content,
            fp.image_url,
            fp.severity,
            fp.severity_reason,
            fp.related_markets,
            fp.source_url,
            fp.posted_at,
            fp.ingested_at
        FROM feed_posts fp
        JOIN feed_sources fs ON fs.id = fp.source_id
        WHERE ($1::text IS NULL OR fs.handle = $1)
          AND ($2::text IS NULL OR fp.severity = $2)
        ORDER BY fp.posted_at DESC
        LIMIT $3
        "#,
        source_handle,
        severity_filter,
        limit
    )
    .fetch_all(pool)
    .await
}

pub async fn get_feed_sources(pool: &PgPool) -> Result<Vec<FeedSource>, sqlx::Error> {
    sqlx::query_as::<_, FeedSource>("SELECT * FROM feed_sources ORDER BY name")
        .fetch_all(pool)
        .await
}

pub async fn add_feed_source(
    pool: &PgPool,
    source_type: &str,
    name: &str,
    handle: &str,
    default_severity: &str,
) -> Result<FeedSource, sqlx::Error> {
    sqlx::query_as::<_, FeedSource>(r#"
        INSERT INTO feed_sources (source_type, name, handle, default_severity)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (source_type, handle) DO UPDATE
            SET name = EXCLUDED.name, updated_at = NOW()
        RETURNING *
    "#)
    .bind(source_type)
    .bind(name)
    .bind(handle.trim_start_matches('@'))
    .bind(default_severity)
    .fetch_one(pool)
    .await
}

pub async fn delete_feed_source(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM feed_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn toggle_feed_source(pool: &PgPool, id: Uuid, enabled: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE feed_sources SET enabled = $1, updated_at = NOW() WHERE id = $2")
        .bind(enabled)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
