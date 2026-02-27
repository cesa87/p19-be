//! RSS/Atom feed ingestion
//!
//! Handles RSS 2.0 and Atom 1.0 feeds stored in `feed_sources` with
//! `source_type = 'rss'` and a non-null `feed_url`.
//!
//! Severity is scored with a fast keyword heuristic — zero API cost.
//! Items older than 48 h are skipped.  Max 25 items per feed per poll.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

// ─── Normalised item ──────────────────────────────────────────────────────────

#[derive(Debug)]
struct Item {
    guid:     String,
    title:    String,
    body:     String,
    link:     String,
    pub_date: DateTime<Utc>,
}

// ─── Public entry points ──────────────────────────────────────────────────────

/// Ingest all enabled `source_type = 'rss'` sources (except CryptoPanic).
pub async fn ingest_rss_sources(pool: &PgPool, http: &Client) {
    let rows = match sqlx::query(
        "SELECT id, handle, feed_url, latitude, longitude \
         FROM feed_sources \
         WHERE enabled = true \
           AND source_type = 'rss' \
           AND handle != 'cryptopanic' \
           AND feed_url IS NOT NULL \
         ORDER BY tier ASC, reliability DESC",
    )
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("RSS: failed to load sources: {}", e);
            return;
        }
    };

    let n_sources = rows.len();
    let mut total_new = 0usize;

    for row in &rows {
        let id:       Uuid        = row.try_get("id").unwrap_or(Uuid::nil());
        let handle:   String      = row.try_get("handle").unwrap_or_default();
        let feed_url: String      = row.try_get("feed_url").unwrap_or_default();
        let lat:      Option<f64> = row.try_get("latitude").ok().flatten();
        let lng:      Option<f64> = row.try_get("longitude").ok().flatten();

        if feed_url.is_empty() {
            continue;
        }

        match ingest_one(pool, http, id, &handle, &feed_url, lat, lng).await {
            Ok(n)  => total_new += n,
            Err(e) => warn!("RSS: {} — {}", handle, e),
        }
    }

    if total_new > 0 {
        info!("📰 RSS: {} sources → {} new posts", n_sources, total_new);
    }
}

/// Ingest the CryptoPanic JSON aggregator feed.
pub async fn ingest_cryptopanic(pool: &PgPool, http: &Client) {
    // Fetch the source record
    let row = match sqlx::query(
        "SELECT id, latitude, longitude FROM feed_sources \
         WHERE source_type = 'rss' AND handle = 'cryptopanic' AND enabled = true \
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(r)) => r,
        _           => return,
    };

    let source_id: Uuid        = row.try_get("id").unwrap_or(Uuid::nil());
    let lat:       Option<f64> = row.try_get("latitude").ok().flatten();
    let lng:       Option<f64> = row.try_get("longitude").ok().flatten();

    #[derive(Deserialize)]
    struct Resp {
        results: Vec<CpPost>,
    }
    #[derive(Deserialize)]
    struct CpPost {
        id:           u64,
        title:        String,
        published_at: String,
        url:          String,
        #[serde(default)]
        currencies:   Vec<CpCcy>,
    }
    #[derive(Deserialize)]
    struct CpCcy {
        code: String,
    }

    let api_url = "https://cryptopanic.com/api/v1/posts/?public=true&kind=news";
    let resp = match http
        .get(api_url)
        .timeout(Duration::from_secs(15))
        .header("User-Agent", "Mozilla/5.0 (compatible; AureumBot/1.0)")
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            warn!("CryptoPanic HTTP {}", r.status());
            return;
        }
        Err(e) => {
            warn!("CryptoPanic fetch: {}", e);
            return;
        }
    };

    let body: Resp = match resp.json().await {
        Ok(b)  => b,
        Err(e) => {
            warn!("CryptoPanic JSON parse: {}", e);
            return;
        }
    };

    let cutoff    = Utc::now() - chrono::Duration::hours(48);
    let mut count = 0usize;

    for post in body.results.into_iter().take(30) {
        let guid     = post.id.to_string();
        let pub_date = parse_date(&post.published_at);
        if pub_date < cutoff {
            continue;
        }

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM feed_posts WHERE source_id=$1 AND external_id=$2)",
        )
        .bind(source_id)
        .bind(&guid)
        .fetch_one(pool)
        .await
        .unwrap_or(true);
        if exists {
            continue;
        }

        let tickers = post
            .currencies
            .iter()
            .map(|c| format!("${}", c.code))
            .collect::<Vec<_>>()
            .join(" ");
        let content = if tickers.is_empty() {
            post.title.clone()
        } else {
            format!("{}\n{}", post.title, tickers)
        };

        let severity = quick_severity(&content);
        let reason   = severity_reason(severity, &content);
        let empty_m: Vec<String> = vec![];

        let _ = sqlx::query(
            "INSERT INTO feed_posts \
             (source_id, external_id, author, content, severity, severity_reason, \
              related_markets, source_url, latitude, longitude, posted_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             ON CONFLICT (source_id, external_id) DO NOTHING",
        )
        .bind(source_id)
        .bind(&guid)
        .bind("CryptoPanic")
        .bind(&content)
        .bind(severity)
        .bind(&reason)
        .bind(&empty_m)
        .bind(Some(&post.url))
        .bind(lat)
        .bind(lng)
        .bind(pub_date)
        .execute(pool)
        .await;

        count += 1;
    }

    if count > 0 {
        info!("💰 CryptoPanic: {} new posts", count);
    }
}

// ─── Per-source ingestion ─────────────────────────────────────────────────────

async fn ingest_one(
    pool:      &PgPool,
    http:      &Client,
    source_id: Uuid,
    handle:    &str,
    feed_url:  &str,
    lat:       Option<f64>,
    lng:       Option<f64>,
) -> Result<usize, String> {
    let resp = http
        .get(feed_url)
        .timeout(Duration::from_secs(15))
        .header("User-Agent", "Mozilla/5.0 (compatible; AureumBot/1.0)")
        .header("Accept", "application/rss+xml, application/atom+xml, text/xml, */*")
        .send()
        .await
        .map_err(|e| format!("HTTP request: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let xml = resp.text().await.map_err(|e| format!("Read body: {}", e))?;
    let items = parse_feed(&xml);

    let cutoff    = Utc::now() - chrono::Duration::hours(48);
    let mut count = 0usize;

    for item in items.into_iter().take(25) {
        if item.pub_date < cutoff {
            continue;
        }

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM feed_posts WHERE source_id=$1 AND external_id=$2)",
        )
        .bind(source_id)
        .bind(&item.guid)
        .fetch_one(pool)
        .await
        .unwrap_or(true);
        if exists {
            continue;
        }

        let content = format!("{}\n\n{}", item.title, item.body);
        let content = content.trim().to_string();
        if content.len() < 20 {
            continue;
        }

        let severity = quick_severity(&content);
        let reason   = severity_reason(severity, &content);
        let empty_m: Vec<String> = vec![];

        let (post_lat, post_lng) = match (lat, lng) {
            (Some(y), Some(x)) => {
                let (jx, jy) = coord_jitter(&item.guid, 1.2);
                (Some(y + jy), Some(x + jx))
            }
            _ => (None, None),
        };

        let link_opt: Option<&str> = if item.link.is_empty() { None } else { Some(&item.link) };

        let _ = sqlx::query(
            "INSERT INTO feed_posts \
             (source_id, external_id, author, content, severity, severity_reason, \
              related_markets, source_url, latitude, longitude, posted_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             ON CONFLICT (source_id, external_id) DO NOTHING",
        )
        .bind(source_id)
        .bind(&item.guid)
        .bind(handle)
        .bind(&content)
        .bind(severity)
        .bind(&reason)
        .bind(&empty_m)
        .bind(link_opt)
        .bind(post_lat)
        .bind(post_lng)
        .bind(item.pub_date)
        .execute(pool)
        .await;

        count += 1;
    }

    Ok(count)
}

// ─── Feed parser ─────────────────────────────────────────────────────────────

fn parse_feed(xml: &str) -> Vec<Item> {
    let is_atom = xml.contains("xmlns=\"http://www.w3.org/2005/Atom\"")
        || (xml.contains("<entry") && xml.contains("<id>"));
    if is_atom {
        parse_atom(xml)
    } else {
        parse_rss(xml)
    }
}

// --- RSS 2.0 -----------------------------------------------------------------

fn parse_rss(xml: &str) -> Vec<Item> {
    split_blocks(xml, "item")
        .into_iter()
        .filter_map(|block| {
            let title = extract_text(&block, "title")?;
            if title.is_empty() {
                return None;
            }
            let desc  = extract_text(&block, "description").unwrap_or_default();
            let link  = extract_text(&block, "link")
                .or_else(|| extract_attr(&block, "link", "href"))
                .unwrap_or_default();
            let guid = extract_text(&block, "guid")
                .filter(|g| !g.is_empty())
                .unwrap_or_else(|| link.clone());
            let raw_date = extract_text(&block, "pubDate")
                .or_else(|| extract_text(&block, "dc:date"))
                .unwrap_or_default();
            Some(Item {
                guid,
                title,
                body:     clean_content(&desc),
                link:     link.trim().to_string(),
                pub_date: parse_date(&raw_date),
            })
        })
        .collect()
}

// --- Atom 1.0 ----------------------------------------------------------------

fn parse_atom(xml: &str) -> Vec<Item> {
    split_blocks(xml, "entry")
        .into_iter()
        .filter_map(|block| {
            let title = extract_text(&block, "title")?;
            if title.is_empty() {
                return None;
            }
            let summary = extract_text(&block, "summary")
                .or_else(|| extract_text(&block, "content"))
                .unwrap_or_default();
            let link    = extract_atom_link(&block);
            let guid    = extract_text(&block, "id")
                .filter(|g| !g.is_empty())
                .unwrap_or_else(|| link.clone());
            let raw_date = extract_text(&block, "published")
                .or_else(|| extract_text(&block, "updated"))
                .unwrap_or_default();
            Some(Item {
                guid,
                title,
                body:     clean_content(&summary),
                link:     link.trim().to_string(),
                pub_date: parse_date(&raw_date),
            })
        })
        .collect()
}

// ─── XML helpers ─────────────────────────────────────────────────────────────

/// Split XML into sub-strings that each contain one `<tag>…</tag>` block.
fn split_blocks(xml: &str, tag: &str) -> Vec<String> {
    let open  = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut blocks = Vec::new();
    let mut pos    = 0usize;
    while let Some(start) = xml[pos..].find(&open) {
        let abs_start    = pos + start;
        let content_start = abs_start + open.len();
        if let Some(end) = xml[content_start..].find(&close) {
            let abs_end = content_start + end;
            blocks.push(xml[content_start..abs_end].to_string());
            pos = abs_end + close.len();
        } else {
            break;
        }
    }
    blocks
}

/// Extract text content of `<tag>…</tag>`, handling CDATA and `<tag …>…</tag>`.
fn extract_text(block: &str, tag: &str) -> Option<String> {
    let close = format!("</{}>", tag);

    // Simple: <tag>…</tag>
    let open = format!("<{}>", tag);
    if let Some(start) = block.find(&open) {
        let inner_start = start + open.len();
        if let Some(end) = block[inner_start..].find(&close) {
            return Some(clean_content(&block[inner_start..inner_start + end]));
        }
    }
    // With attributes: <tag …>…</tag>
    let open_with_attrs = format!("<{} ", tag);
    if let Some(tag_start) = block.find(&open_with_attrs) {
        if let Some(gt) = block[tag_start..].find('>') {
            let inner_start = tag_start + gt + 1;
            if let Some(end) = block[inner_start..].find(&close) {
                return Some(clean_content(&block[inner_start..inner_start + end]));
            }
        }
    }
    None
}

/// Extract an attribute value from `<tag attr="…" …>`.
fn extract_attr(block: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_prefix  = format!("<{}", tag);
    let attr_needle = format!("{}=\"", attr);
    let start       = block.find(&tag_prefix)?;
    let tag_end     = block[start..].find('>')? + 1;
    let tag_str     = &block[start..start + tag_end];
    let attr_start  = tag_str.find(&attr_needle)?;
    let val_start   = attr_start + attr_needle.len();
    let val_end     = tag_str[val_start..].find('"')?;
    Some(tag_str[val_start..val_start + val_end].to_string())
}

/// Best link from an Atom `<entry>`: prefers `rel="alternate"`.
fn extract_atom_link(block: &str) -> String {
    let mut best: Option<String> = None;
    let mut pos = 0usize;
    while let Some(start) = block[pos..].find("<link") {
        let abs    = pos + start;
        let after  = &block[abs..];
        let tag_end = after.find('>').unwrap_or(after.len() - 1);
        let tag     = &after[..tag_end + 1];
        let href    = extract_quoted(tag, "href");
        let rel     = extract_quoted(tag, "rel").unwrap_or_default();
        if let Some(h) = href {
            if rel == "alternate" {
                return h;
            }
            if best.is_none() {
                best = Some(h);
            }
        }
        pos = abs + 5;
    }
    best.unwrap_or_default()
}

fn extract_quoted(s: &str, attr: &str) -> Option<String> {
    let needle    = format!("{}=\"", attr);
    let start     = s.find(&needle)?;
    let val_start = start + needle.len();
    let val_end   = s[val_start..].find('"')?;
    Some(s[val_start..val_start + val_end].to_string())
}

// ─── Content cleaning ─────────────────────────────────────────────────────────

/// Strip CDATA, HTML tags, decode entities, normalise whitespace.
fn clean_content(raw: &str) -> String {
    let s = raw.trim();
    // Unwrap CDATA
    let s: &str = if let Some(inner) = s.strip_prefix("<![CDATA[") {
        inner.strip_suffix("]]>").unwrap_or(inner)
    } else {
        s
    };
    let s = strip_html(s);
    let s = decode_entities(&s);
    // Collapse whitespace
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_html(s: &str) -> String {
    let mut out    = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => { in_tag = true; out.push(' '); }
            '>' => { in_tag = false; }
            _   => { if !in_tag { out.push(ch); } }
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;",   "&")
     .replace("&lt;",    "<")
     .replace("&gt;",    ">")
     .replace("&quot;",  "\"")
     .replace("&apos;",  "'")
     .replace("&#39;",   "'")
     .replace("&nbsp;",  " ")
     .replace("&mdash;", "\u{2014}")
     .replace("&ndash;", "\u{2013}")
     .replace("&hellip;","\u{2026}")
     .replace("&laquo;", "\u{00AB}")
     .replace("&raquo;", "\u{00BB}")
     .replace("&#8216;", "\u{2018}")
     .replace("&#8217;", "\u{2019}")
     .replace("&#8220;", "\u{201C}")
     .replace("&#8221;", "\u{201D}")
}

// ─── Date parsing ─────────────────────────────────────────────────────────────

fn parse_date(s: &str) -> DateTime<Utc> {
    let s = s.trim();
    if s.is_empty() {
        return Utc::now();
    }
    // RFC 2822 (RSS 2.0): "Thu, 26 Feb 2026 23:00:00 +0000"
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(s) {
        return dt.with_timezone(&Utc);
    }
    // RFC 3339 / ISO 8601 (Atom): "2026-02-26T23:00:00Z"
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc);
    }
    // Truncated ISO: "2026-02-26T23:00:00"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return dt.and_utc();
    }
    // Space-separated: "2026-02-26 23:00:00"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return dt.and_utc();
    }
    Utc::now()
}

// ─── Severity scoring ─────────────────────────────────────────────────────────

fn quick_severity(content: &str) -> &'static str {
    let c = content.to_lowercase();

    // CRITICAL — systemic shocks
    const CRITICAL: &[&str] = &[
        "declares war",
        "declaration of war",
        "nuclear strike",
        "nuclear attack",
        "federal reserve raises",
        "fed raises rates",
        "emergency rate cut",
        "circuit breaker triggered",
        "trading halted",
        "exchange collapsed",
        "exchange hacked",
        "funds stolen",
        "assassination of",
        "government collapsed",
        "martial law declared",
        "coup d",
    ];
    for kw in CRITICAL {
        if c.contains(kw) {
            return "CRITICAL";
        }
    }

    // HIGH — major actionable signals
    const HIGH: &[&str] = &[
        // Breaking news formats
        "breaking news",
        "breaking:",
        "just in:",
        "flash:",
        // Military/kinetic
        "airstrike",
        "airstrikes",
        "missile strike",
        "missile strikes",
        "rocket attack",
        "rocket attacks",
        "launches attack",
        "launches attacks",
        "launches strikes",
        "launches offensive",
        "aerial bombardment",
        "aerial attack",
        "aerial strikes",
        "bombards",
        "shelled",
        "explosion in",
        "bombing of",
        "killed in",
        "kills in",
        "dead in",
        "mass casualty",
        "ceasefire violation",
        "violates ceasefire",
        "ground offensive",
        "invades",
        // Elections/political
        "election result",
        "wins the election",
        "elected president",
        "wins election",
        // Sanctions/trade
        "new sanctions",
        "sanctions imposed",
        "tariffs on",
        "trade war",
        // Legal
        "arrested",
        "indicted",
        "charged with",
        // Economic data
        "cpi report",
        "inflation data",
        "jobs report",
        "nonfarm payroll",
        "federal reserve decision",
        "rate decision",
        // Markets/corporate
        "sec charges",
        "sec lawsuit",
        "crypto ban",
        "bitcoin etf approved",
        "bankruptcy filed",
        "chapter 11",
        // M&A — these move prediction markets
        "acquires",
        "acquisition of",
        "to acquire",
        "merger approved",
        "deal superior",
        "deal agreed",
        "deal closed",
        "takeover bid",
        "hostile bid",
        // Diplomacy/nuclear
        "nuclear deal",
        "nuclear agreement",
        "nuclear talks",
        "significant progress",
        "deal reached",
        "agreement signed",
        // Geopolitical escalation
        "emergency summit",
        "mobilizes",
        "deploys troops",
        "sends troops",
        "military buildup",
        "open war",
    ];
    for kw in HIGH {
        if c.contains(kw) {
            return "HIGH";
        }
    }

    // MEDIUM — notable, market-relevant
    const MEDIUM: &[&str] = &[
        "election",
        "vote",
        "referendum",
        "poll shows",
        "warning",
        "alert:",
        "volatile",
        "tensions",
        "deal signed",
        "agreement reached",
        "ceasefire",
        "treaty",
        "protest",
        "riot",
        "earnings",
        "quarterly results",
        "revenue",
        "gdp",
        "unemployment",
        "federal reserve",
        "bitcoin",
        "ethereum",
        "crypto",
        "oil price",
        "crude oil",
        "gas prices",
        "sanctions",
        "tariffs",
        "impeach",
        "resign",
    ];
    for kw in MEDIUM {
        if c.contains(kw) {
            return "MEDIUM";
        }
    }

    "LOW"
}

fn severity_reason(severity: &str, content: &str) -> String {
    if severity == "LOW" {
        return "RSS feed item – no high-priority keywords detected".to_string();
    }
    let c = content.to_lowercase();
    let triggers: &[&str] = match severity {
        "CRITICAL" => &["war", "nuclear", "rate", "collapse", "assassination", "coup", "martial"],
        "HIGH"     => &["breaking", "airstrike", "missile", "killed", "election", "sanction", "tariff", "arrested", "cpi", "bankruptcy", "indicted"],
        _          => &["election", "bitcoin", "protest", "earnings", "gdp", "sanction", "oil", "crypto"],
    };
    for t in triggers {
        if c.contains(t) {
            return format!("Keyword match: \"{}\" → {}", t, severity);
        }
    }
    format!("RSS feed item – scored {}", severity)
}

// ─── Coord jitter (mirrors feed.rs) ──────────────────────────────────────────

fn coord_jitter(seed: &str, range: f64) -> (f64, f64) {
    let mut h: u64 = 5381;
    for b in seed.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    let jx = ((h % 1000) as f64 / 1000.0 - 0.5) * range * 2.0;
    let jy = (((h >> 16) % 1000) as f64 / 1000.0 - 0.5) * range * 2.0;
    (jx, jy)
}
