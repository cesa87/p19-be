//! Market Movers — tracks price changes across Polymarket markets.
//!
//! Background job polls top-200 markets every 5 minutes and stores price
//! snapshots. The movers endpoint returns markets with the biggest price
//! movements over the requested time window.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::intelligence::markets::{MarketCard, MarketsFetcher};

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MarketMover {
    pub rank:           usize,
    pub market_id:      String,
    /// Polymarket hex condition_id (0x...) — used by CLOB API for trading
    pub condition_id:   String,
    pub title:          String,
    pub url:            String,
    pub image_url:      Option<String>,
    /// The outcome that moved most (e.g. "YES", "NO", team name)
    pub outcome_moved:  String,
    /// "UP" or "DOWN"
    pub direction:      String,
    pub detected_at:    DateTime<Utc>,
    pub detected_price: f64,
    pub peak_price:     f64,
    pub peak_at:        DateTime<Utc>,
    pub current_price:  f64,
    /// % change from detected_price to peak_price
    pub change_pct:     f64,
    pub volume_usd:     f64,
    /// Feed post that likely triggered the move (title + source)
    pub signal_content: Option<String>,
    pub signal_source:  Option<String>,
    pub signal_url:     Option<String>,
    pub signal_severity: Option<String>,
    /// Seconds between first matching news post and market move detection
    pub signal_lag_secs: Option<i64>,
}

// ─── Tracker ─────────────────────────────────────────────────────────────────

pub struct MarketSnapshotTracker {
    pool:    PgPool,
    fetcher: MarketsFetcher,
}

impl MarketSnapshotTracker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, fetcher: MarketsFetcher::new() }
    }

    /// Fetch top markets by volume and store a price snapshot for each outcome.
    pub async fn take_snapshot(&self) {
        match self.fetcher.fetch(None, "volume", 200).await {
            Ok(markets) => {
                let outcome_count: usize = markets.iter().map(|m| m.outcomes.len()).sum();
                self.store_snapshots(&markets).await;
                // Prune snapshots older than 48 h to keep the table lean.
                let _ = sqlx::query!(
                    "DELETE FROM market_snapshots WHERE captured_at < NOW() - INTERVAL '48 hours'"
                )
                .execute(&self.pool)
                .await;
                info!("Market snapshot: stored {} outcomes from {} markets", outcome_count, markets.len());
            }
            Err(e) => warn!("Market snapshot fetch failed: {}", e),
        }
    }

    async fn store_snapshots(&self, markets: &[MarketCard]) {
        for market in markets {
            for outcome in &market.outcomes {
                let _ = sqlx::query!(
                    "INSERT INTO market_snapshots
                         (market_id, condition_id, title, outcome_name, price, volume_usd, image_url, url)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    market.id,
                    market.condition_id,
                    market.title,
                    outcome.name,
                    outcome.price,
                    market.volume_usd,
                    market.image_url,
                    market.url,
                )
                .execute(&self.pool)
                .await;
            }
        }
    }

    /// Return the top `limit` movers over the last `hours` hours, sorted by
    /// biggest price swing (detected → peak).
    pub async fn get_movers(&self, hours: i32, limit: i64) -> Result<Vec<MarketMover>, anyhow::Error> {
        use sqlx::Row;

        let rows = sqlx::query(
            r#"
            WITH window_snaps AS (
                SELECT * FROM market_snapshots
                WHERE captured_at > NOW() - (($1::int) * INTERVAL '1 hour')
            ),
            bounds AS (
                SELECT
                    market_id,
                    outcome_name,
                    MIN(captured_at) AS first_at,
                    MAX(captured_at) AS last_at
                FROM window_snaps
                GROUP BY market_id, outcome_name
                HAVING COUNT(*) >= 2
            ),
            with_prices AS (
                SELECT
                    b.market_id,
                    b.outcome_name,
                    sl.title,
                    sl.image_url,
                    sl.url,
                    sl.condition_id,
                    sf.price       AS start_price,
                    sf.captured_at AS detected_at,
                    sl.price       AS current_price,
                    sl.volume_usd  AS volume_usd
                FROM bounds b
                JOIN window_snaps sf
                    ON sf.market_id    = b.market_id
                   AND sf.outcome_name = b.outcome_name
                   AND sf.captured_at  = b.first_at
                JOIN window_snaps sl
                    ON sl.market_id    = b.market_id
                   AND sl.outcome_name = b.outcome_name
                   AND sl.captured_at  = b.last_at
            ),
            with_peak AS (
                SELECT
                    wp.*,
                    pk.price       AS peak_price,
                    pk.captured_at AS peak_at
                FROM with_prices wp
                CROSS JOIN LATERAL (
                    SELECT price, captured_at
                    FROM window_snaps ws
                    WHERE ws.market_id    = wp.market_id
                      AND ws.outcome_name = wp.outcome_name
                    ORDER BY ws.price DESC
                    LIMIT 1
                ) pk
            )
            SELECT
                market_id,
                condition_id,
                outcome_name,
                title,
                image_url,
                url,
                start_price,
                detected_at,
                peak_price,
                peak_at,
                current_price,
                volume_usd,
                CASE WHEN start_price > 0.0001
                     THEN (peak_price - start_price) / start_price * 100.0
                     ELSE 0.0
                END AS change_pct
            FROM with_peak
            WHERE start_price BETWEEN 0.10 AND 0.92
              AND peak_price - start_price >= 0.03
              AND volume_usd >= 500
              AND title NOT ILIKE '%spread:%'
              AND title NOT ILIKE '%o/u%'
              AND title NOT ILIKE '%over/under%'
              AND title NOT ILIKE '%handicap%'
            ORDER BY change_pct DESC
            LIMIT $2
            "#,
        )
        .bind(hours)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let movers: Vec<MarketMover> = rows
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let detected_price: f64 = row.try_get("start_price").unwrap_or(0.0);
                let peak_price:     f64 = row.try_get("peak_price").unwrap_or(detected_price);
                let current_price:  f64 = row.try_get("current_price").unwrap_or(detected_price);
                let change_pct:     f64 = row.try_get("change_pct").unwrap_or(0.0);
                let detected_at: DateTime<Utc> = row.try_get("detected_at").unwrap_or_else(|_| Utc::now());
                let peak_at:     DateTime<Utc> = row.try_get("peak_at").unwrap_or_else(|_| Utc::now());
                let direction = if peak_price >= detected_price { "UP" } else { "DOWN" }.to_string();

                MarketMover {
                    rank:           i + 1,
                    market_id:      row.try_get("market_id").unwrap_or_default(),
                    condition_id:   row.try_get("condition_id").unwrap_or_default(),
                    title:          row.try_get("title").unwrap_or_default(),
                    url:            row.try_get::<Option<String>, _>("url").ok().flatten().unwrap_or_default(),
                    image_url:      row.try_get("image_url").ok().flatten(),
                    outcome_moved:  row.try_get("outcome_name").unwrap_or_default(),
                    direction,
                    detected_at,
                    detected_price,
                    peak_price,
                    peak_at,
                    current_price,
                    change_pct,
                    volume_usd: row.try_get("volume_usd").unwrap_or(0.0),
                    signal_content:  None,
                    signal_source:   None,
                    signal_url:      None,
                    signal_severity: None,
                    signal_lag_secs: None,
                }
            })
            .collect();

        // Enrich each mover with the first relevant news signal
        let mut movers_enriched = Vec::with_capacity(movers.len());
        for mut mover in movers {
            if let Ok(sig) = self.find_signal_for_mover(&mover).await {
                mover.signal_content  = sig.0;
                mover.signal_source   = sig.1;
                mover.signal_url      = sig.2;
                mover.signal_severity = sig.3;
                mover.signal_lag_secs = sig.4;
            }
            movers_enriched.push(mover);
        }
        Ok(movers_enriched)
    }

    /// Find the earliest relevant feed post for a mover.
    /// Looks for posts containing >=2 title keywords posted up to 4h before detection.
    async fn find_signal_for_mover(
        &self,
        mover: &MarketMover,
    ) -> Result<(Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>), anyhow::Error> {
        use sqlx::Row;

        // Extract keywords from title (>=5 chars, simple split)
        let stopwords = ["will","the","a","an","in","on","by","of","to","for","from","is","are",
                         "was","were","that","this","with","and","or","not","no","yes","be","at",
                         "its","what","when","where","how","who","which","any","all","before",
                         "after","during","within","about","2024","2025","2026","2027","close",
                         "end","day","week","month","year","date","time","reach","hit","make",
                         "take","give","open","hold","move","rise","fall","sign","news","says"];

        let keywords: Vec<String> = mover.title
            .split_whitespace()
            .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| w.len() >= 5 && !stopwords.contains(&w.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if keywords.len() < 2 {
            return Ok((None, None, None, None, None));
        }

        // Build WHERE clause: content must contain at least 2 keywords
        // We use ILIKE for each and sum matches
        let kw_conditions: Vec<String> = keywords.iter()
            .map(|kw| format!("(CASE WHEN LOWER(fp.content) LIKE '%{}%' THEN 1 ELSE 0 END)", kw))
            .collect();
        let kw_sum = kw_conditions.join(" + ");

        let sql = format!(
            "SELECT fp.content, fs.name, fp.source_url, fp.severity,
                    EXTRACT(EPOCH FROM ($1::timestamptz - fp.posted_at))::BIGINT AS lag_secs
             FROM feed_posts fp
             JOIN feed_sources fs ON fs.id = fp.source_id
             WHERE fp.posted_at BETWEEN $1 - INTERVAL '6 hours' AND $1 + INTERVAL '30 minutes'
               AND ({kw_sum}) >= 2
             ORDER BY fp.posted_at ASC
             LIMIT 1",
            kw_sum = kw_sum
        );

        let row = sqlx::query(&sql)
            .bind(mover.detected_at)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            None => Ok((None, None, None, None, None)),
            Some(r) => {
                let content: String  = r.try_get("content").unwrap_or_default();
                let name:    String  = r.try_get("name").unwrap_or_default();
                let url:     Option<String> = r.try_get("source_url").ok().flatten();
                let sev:     String  = r.try_get("severity").unwrap_or_default();
                let lag:     i64     = r.try_get("lag_secs").unwrap_or(0);
                // Truncate content to 280 chars for display
                let snippet: String  = content.chars().take(280).collect();
                Ok((Some(snippet), Some(name), url, Some(sev), Some(lag)))
            }
        }
    }

    /// Background scheduler — runs every 5 minutes.
    pub async fn run_scheduler(&self) {
        use tokio::time::{interval, Duration};
        // Take an immediate snapshot on startup.
        self.take_snapshot().await;
        let mut ticker = interval(Duration::from_secs(300));
        loop {
            ticker.tick().await;
            self.take_snapshot().await;
        }
    }
}
