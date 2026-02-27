//! Signal Intelligence — matches breaking feed posts to Polymarket markets.
//!
//! When a Telegram/news post mentions entities that match a Polymarket market
//! title but the market price hasn't moved yet, we surface it as an actionable
//! signal window. Scoring weights: keyword coverage × source reliability ×
//! recency × price stability (window still open) × corroboration.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

// ─── Public return types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SignalPost {
    pub source_handle:     String,
    pub source_reliability: f64,
    pub source_tier:       i32,
    pub snippet:           String,   // up to 280 chars
    pub severity:          String,
    pub posted_at:         String,
    pub source_url:        Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignalMatch {
    pub market_id:         String,
    pub market_title:      String,
    pub market_url:        String,
    pub image_url:         Option<String>,
    /// Which outcome the signal is relevant to
    pub outcome_name:      String,
    pub current_price:     f64,
    /// % change vs 24 h ago (None if no history)
    pub price_change_24h:  Option<f64>,
    /// True = market hasn't repriced yet → window still open
    pub price_stable:      bool,
    /// 0–100 conviction score
    pub match_score:       f64,
    /// Specific title keywords found in the feed posts
    pub matched_keywords:  Vec<String>,
    /// Number of independent sources corroborating
    pub source_count:      usize,
    pub signal_posts:      Vec<SignalPost>,
    /// ISO timestamp of the earliest matching post
    pub first_signal_at:   String,
}

// ─── Internal query rows ──────────────────────────────────────────────────────

struct MarketRow {
    market_id:       String,
    title:           String,
    outcome_name:    String,
    price:           f64,
    image_url:       Option<String>,
    url:             Option<String>,
    price_change_24h: Option<f64>,
    price_stable:    bool,
}

struct PostRow {
    source_handle:  String,
    reliability:    f64,
    tier:           i32,
    content:        String,
    severity:       String,
    source_url:     Option<String>,
    posted_at:      DateTime<Utc>,
    age_secs:       i64,
}

// ─── Engine ───────────────────────────────────────────────────────────────────

pub struct SignalEngine {
    pool: PgPool,
}

impl SignalEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_signals(
        &self,
        feed_hours: i32,
        limit: i64,
    ) -> Result<Vec<SignalMatch>, anyhow::Error> {
        let markets = self.latest_market_prices().await?;
        let posts   = self.recent_feed_posts(feed_hours).await?;

        if markets.is_empty() || posts.is_empty() {
            return Ok(vec![]);
        }

        let mut matches: Vec<SignalMatch> = Vec::new();

        for market in &markets {
            let keywords = extract_keywords(&market.title);
            if keywords.len() < 2 {
                continue; // Need at least 2 meaningful keywords
            }

            // Find every post that hits at least one keyword
            let mut hits: Vec<(&PostRow, Vec<String>)> = Vec::new();
            for post in &posts {
                let content_lc = post.content.to_lowercase();
                let hit_kws: Vec<String> = keywords.iter()
                    .filter(|kw| content_lc.contains(kw.as_str()))
                    .cloned()
                    .collect();
                // Require at least 2 keywords in the SAME post — prevents
                // single-word noise matches (e.g. "post" in an unrelated text)
                if hit_kws.len() >= 2 {
                    hits.push((post, hit_kws));
                }
            }

            if hits.is_empty() {
                continue;
            }

            // Deduplicate matched keywords across all posts
            let mut all_hit_kws: Vec<String> = {
                let mut set = std::collections::HashSet::new();
                for (_, kws) in &hits { for kw in kws { set.insert(kw.clone()); } }
                let mut v: Vec<String> = set.into_iter().collect();
                v.sort();
                v
            };

            let max_reliability = hits.iter().map(|(p, _)| p.reliability).fold(0f64, f64::max);
            let min_age_secs    = hits.iter().map(|(p, _)| p.age_secs).min().unwrap_or(99999);
            let num_posts       = hits.len();

            let score = compute_score(
                all_hit_kws.len(),
                keywords.len(),
                max_reliability,
                min_age_secs,
                market.price_stable,
                num_posts,
            );

            if score < 4.0 {
                continue; // Filter noise
            }

            let first_post = hits.iter()
                .min_by_key(|(p, _)| p.age_secs)
                .unwrap();

            let signal_posts: Vec<SignalPost> = hits.iter().map(|(p, _)| SignalPost {
                source_handle:     p.source_handle.clone(),
                source_reliability: p.reliability,
                source_tier:       p.tier,
                snippet:           p.content.chars().take(280).collect(),
                severity:          p.severity.clone(),
                posted_at:         p.posted_at.to_rfc3339(),
                source_url:        p.source_url.clone(),
            }).collect();

            matches.push(SignalMatch {
                market_id:        market.market_id.clone(),
                market_title:     market.title.clone(),
                market_url:       market.url.clone().unwrap_or_default(),
                image_url:        market.image_url.clone(),
                outcome_name:     market.outcome_name.clone(),
                current_price:    market.price,
                price_change_24h: market.price_change_24h,
                price_stable:     market.price_stable,
                match_score:      (score * 10.0).round() / 10.0,
                matched_keywords: all_hit_kws,
                source_count:     num_posts,
                signal_posts,
                first_signal_at:  first_post.0.posted_at.to_rfc3339(),
            });
        }

        matches.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score)
            .unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(limit as usize);
        Ok(matches)
    }

    /// Latest price per market+outcome from snapshots
    async fn latest_market_prices(&self) -> Result<Vec<MarketRow>, anyhow::Error> {
        use sqlx::Row;

        let rows = sqlx::query(r#"
            WITH latest AS (
                SELECT DISTINCT ON (market_id, outcome_name)
                    market_id, title, outcome_name, price, image_url, url
                FROM market_snapshots
                ORDER BY market_id, outcome_name, captured_at DESC
            ),
            day_ago AS (
                SELECT DISTINCT ON (market_id, outcome_name)
                    market_id, outcome_name, price AS day_ago_price
                FROM market_snapshots
                WHERE captured_at BETWEEN NOW() - INTERVAL '25 hours'
                                      AND NOW() - INTERVAL '23 hours'
                ORDER BY market_id, outcome_name, captured_at DESC
            )
            SELECT
                l.market_id,
                l.title,
                l.outcome_name,
                l.price,
                l.image_url,
                l.url,
                CASE WHEN d.day_ago_price > 0
                     THEN (l.price - d.day_ago_price) / d.day_ago_price * 100.0
                     ELSE NULL END AS price_change_24h,
                ABS(l.price - COALESCE(d.day_ago_price, l.price)) < 0.06
                    AS price_stable
            FROM latest l
            LEFT JOIN day_ago d USING (market_id, outcome_name)
            WHERE l.price BETWEEN 0.06 AND 0.94
        "#)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| MarketRow {
            market_id:       r.try_get("market_id").unwrap_or_default(),
            title:           r.try_get("title").unwrap_or_default(),
            outcome_name:    r.try_get("outcome_name").unwrap_or_default(),
            price:           r.try_get("price").unwrap_or(0.0),
            image_url:       r.try_get("image_url").ok().flatten(),
            url:             r.try_get("url").ok().flatten(),
            price_change_24h: r.try_get("price_change_24h").ok().flatten(),
            price_stable:    r.try_get("price_stable").unwrap_or(true),
        }).collect())
    }

    /// Recent feed posts with source metadata
    async fn recent_feed_posts(&self, hours: i32) -> Result<Vec<PostRow>, anyhow::Error> {
        use sqlx::Row;

        let rows = sqlx::query(r#"
            SELECT
                fs.handle                                          AS source_handle,
                fs.reliability,
                fs.tier,
                fp.content,
                fp.severity,
                fp.source_url,
                fp.posted_at,
                EXTRACT(EPOCH FROM (NOW() - fp.posted_at))::BIGINT AS age_secs
            FROM feed_posts fp
            JOIN feed_sources fs ON fs.id = fp.source_id
            WHERE fp.posted_at > NOW() - ($1::int * INTERVAL '1 hour')
              AND fs.enabled = true
              AND LENGTH(fp.content) >= 30
            ORDER BY fp.posted_at DESC
            LIMIT 500
        "#)
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| PostRow {
            source_handle: r.try_get("source_handle").unwrap_or_default(),
            reliability:   r.try_get("reliability").unwrap_or(0.7),
            tier:          r.try_get::<i32, _>("tier").unwrap_or(2),
            content:       r.try_get("content").unwrap_or_default(),
            severity:      r.try_get("severity").unwrap_or_default(),
            source_url:    r.try_get("source_url").ok().flatten(),
            posted_at:     r.try_get("posted_at").unwrap_or_else(|_| Utc::now()),
            age_secs:      r.try_get("age_secs").unwrap_or(99999),
        }).collect())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const STOPWORDS: &[&str] = &[
    "will","the","a","an","in","on","by","of","to","for","from","is","are",
    "was","were","that","this","with","and","or","not","no","yes","be","at",
    "its","it","have","has","had","would","could","should","may","might",
    "into","over","under","above","below","between","than","more","less",
    "most","least","new","old","high","low","long","short","big","small",
    "close","end","day","week","month","year","date","time","what","when",
    "where","how","who","which","any","all","before","after","during",
    "within","about","up","down","out","off","back","first","last","next",
    "per","get","hit","reach","make","take","give","go","come","through",
    "against","some","these","those","been","they","their","them","each",
    "many","much","such","only","also","just","then","does","did","doing",
    "done","being","2024","2025","2026","2027","january","february","march",
    "april","june","july","august","september","october","november","december",
    "lose","beat","wins","win","open","close","hold","keep","move","rise",
    "fall","sign","from","with","have","been","than","that","this","there",
    // Generic/noise words that cause false positives
    "post","posts","tweet","tweets","both","team","game","match","play",
    "launch","token","coin","star","lead","dead","live","news","says","said",
    "seen","show","shows","show","site","type","even","well","know","make",
    "take","give","goes","come","puts","puts","left","right","real","true",
    "false","could","would","should","2028","2029","2030","2031","2032",
];

fn extract_keywords(title: &str) -> Vec<String> {
    let mut set = std::collections::HashSet::new();
    for word in title.split_whitespace() {
        let w = word.to_lowercase();
        let w = w.trim_matches(|c: char| !c.is_alphanumeric());
        if w.len() >= 5 && !STOPWORDS.contains(&w) {
            set.insert(w.to_string());
        }
    }
    set.into_iter().collect()
}

fn compute_score(
    hit_kw_count:   usize,
    total_kw_count: usize,
    max_reliability: f64,
    min_age_secs:   i64,
    price_stable:   bool,
    num_posts:      usize,
) -> f64 {
    if total_kw_count == 0 || hit_kw_count == 0 { return 0.0; }

    // Coverage: what fraction of the market's keywords appear in the post
    let coverage = hit_kw_count as f64 / total_kw_count.max(1) as f64;

    // Recency: exponential decay, half-life = 3 hours
    let recency = (-min_age_secs as f64 / 10_800.0).exp();

    // Source quality
    let quality = max_reliability.clamp(0.3, 1.0);

    // Window bonus: if price hasn't moved yet the arb window is still open
    let window = if price_stable { 1.5 } else { 0.7 };

    // Corroboration: each additional source adds 25% conviction
    let corroboration = 1.0 + (num_posts as f64 - 1.0).max(0.0) * 0.25;

    coverage * recency * quality * window * corroboration * 100.0
}
