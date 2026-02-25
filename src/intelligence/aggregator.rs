//! Feed Aggregator — polls all data sources and normalises into FeedSnapshots

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use crate::feeds::{
    vix::VixClient,
    fear_greed::FearGreedClient,
    fred::FredClient,
    dxy::DxyClient,
    reddit_sentiment::RedditSentimentClient,
    alternative_me::AlternativeMeClient,
    gld::GldClient,
};
use crate::macro_sentiment::polymarket::PolymarketClient;
use crate::news::{
    alphavantage::AlphaVantageClient,
    finnhub::FinnhubClient,
    newsapi::NewsApiClient,
};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSnapshot {
    pub feed_id: String,
    pub feed_source: String,
    pub value: f64,        // Normalised 0-100 where applicable, otherwise raw
    pub label: String,
    pub raw_json: Option<serde_json::Value>,
    pub fetched_at: DateTime<Utc>,
}

impl FeedSnapshot {
    pub fn new(feed_id: &str, feed_source: &str, value: f64, label: &str) -> Self {
        Self {
            feed_id: feed_id.to_string(),
            feed_source: feed_source.to_string(),
            value,
            label: label.to_string(),
            raw_json: None,
            fetched_at: Utc::now(),
        }
    }
}

pub struct IntelligenceAggregator {
    pool: PgPool,
    config: Arc<Config>,
}

impl IntelligenceAggregator {
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self { pool, config }
    }

    /// Run the aggregator on a schedule (every 5 minutes)
    pub async fn run_scheduler(self: Arc<Self>) {
        info!("🧠 Intelligence aggregator starting...");
        let mut ticker = interval(Duration::from_secs(300)); // 5 min
        loop {
            ticker.tick().await;
            if let Err(e) = self.collect_all_feeds().await {
                warn!("Intelligence feed collection error: {}", e);
            }
        }
    }

    /// Collect all feeds and store snapshots
    pub async fn collect_all_feeds(&self) -> Result<Vec<FeedSnapshot>> {
        let mut snapshots = Vec::new();

        // ── VIX ────────────────────────────────────────────────────────────
        let vix_client = VixClient::new();
        if let Some(vix) = vix_client.get_vix().await {
            let fear_score = VixClient::vix_to_fear_score(vix);
            let label = format!("VIX: {:.1} — {}", vix, vix_label(vix));
            snapshots.push(FeedSnapshot::new("vix", "cboe", fear_score, &label));
        }

        // ── Stock Fear & Greed (CNN) ───────────────────────────────────────
        let fg_client = FearGreedClient::new();
        if let Some(fg) = fg_client.get_fear_greed().await {
            let label = format!("Fear & Greed: {} ({})", fg.value, fg.classification);
            snapshots.push(FeedSnapshot::new("fear_greed_stocks", "cnn", fg.value as f64, &label));
        }

        // ── Crypto Fear & Greed (Alternative.me) ──────────────────────────
        let altme = AlternativeMeClient::new();
        if let Some(fg) = altme.get_fear_greed().await {
            let label = format!("Crypto Fear & Greed: {} ({})", fg.value, fg.value_classification);
            snapshots.push(FeedSnapshot::new("fear_greed_crypto", "alternative_me", fg.value as f64, &label));
        }

        // ── DXY (Dollar Index) ────────────────────────────────────────────
        let dxy_client = DxyClient::new();
        if let Some(dxy) = dxy_client.get_dxy().await {
            // DXY inverse relationship with Gold: normalise 95-110 → 0-100
            let dxy_score = ((dxy.level - 95.0) / 15.0 * 100.0).clamp(0.0, 100.0);
            let trend = DxyClient::dxy_trend(dxy.level, dxy.sma_5d);
            let label = format!("DXY: {:.2} ({}) — Gold pressure: {:.0}", dxy.level, trend, dxy_score);
            let mut snap = FeedSnapshot::new("dxy", "yahoo_finance", dxy_score, &label);
            snap.raw_json = Some(serde_json::json!({ "current": dxy.level, "sma": dxy.sma_5d, "trend": trend }));
            snapshots.push(snap);
        }

        // ── FRED (Treasury / Real Yield) ──────────────────────────────────
        let fred_client = FredClient::new();
        let treasury = fred_client.get_treasury_data().await;
        if let Some(real_yield) = treasury.real_yield_10y {
            let gold_score = FredClient::real_yield_to_gold_score(real_yield);
            let label = format!("Real Yield 10Y: {:.2}% — Gold score: {:.0}", real_yield, gold_score);
            let mut snap = FeedSnapshot::new("real_yield_10y", "fred", gold_score, &label);
            snap.raw_json = Some(serde_json::json!({ "real_yield_10y": real_yield, "yield_10y": treasury.yield_10y, "yield_2y": treasury.yield_2y }));
            snapshots.push(snap);
        }
        if let (Some(y10), Some(y2)) = (treasury.yield_10y, treasury.yield_2y) {
            let slope = FredClient::yield_curve_slope(y10, y2);
            // Inverted yield curve (negative slope) = high tension
            let tension = if slope < 0.0 { 80.0 } else if slope < 0.5 { 50.0 } else { 20.0 };
            let label = format!("Yield Curve: {:.2}% (10Y-2Y) — {}", slope, if slope < 0.0 { "INVERTED ⚠️" } else { "Normal" });
            snapshots.push(FeedSnapshot::new("yield_curve_slope", "fred", tension, &label));
        }

        // ── Reddit Sentiment ──────────────────────────────────────────────
        let reddit = RedditSentimentClient::new();
        if let Some(rs) = reddit.get_sentiment().await {
            // Normalise: bullish_ratio 0-1 → 0-100
            let score = if rs.bullish_consensus { 70.0 } else if rs.bearish_consensus { 30.0 } else { (50.0 + rs.btc_sentiment * 25.0).clamp(0.0, 100.0) };
            let label = format!("Reddit: {:.0}% bullish ({} posts, {:.2} btc sentiment)", score, rs.posts_analyzed, rs.btc_sentiment);
            let mut snap = FeedSnapshot::new("reddit_sentiment", "reddit", score, &label);
            snap.raw_json = Some(serde_json::json!({ "btc_sentiment": rs.btc_sentiment, "posts_analyzed": rs.posts_analyzed, "bullish_consensus": rs.bullish_consensus }));
            snapshots.push(snap);
        }

        // ── GLD ETF Flows ─────────────────────────────────────────────────
        let gld = GldClient::new();
        if let Some(flow) = gld.get_gld_flow().await {
            // Positive flow = demand, negative = outflow
            let score = ((flow.volume_ratio - 0.7) / 1.3 * 100.0).clamp(0.0, 100.0);
            let label = format!("GLD ETF: {:.2}x vol ratio ({:.0}K vol)", flow.volume_ratio, flow.current_volume / 1_000.0);
            snapshots.push(FeedSnapshot::new("gld_flow", "yahoo_finance", score, &label));
        }

        // ── Polymarket ────────────────────────────────────────────────────
        let poly = PolymarketClient::new();
        let macro_sentiment = poly.calculate_macro_sentiment().await;
        
        // Fed uncertainty (high uncertainty = tension)
        let fed_tension = macro_sentiment.fed_uncertainty * 100.0;
        let label = format!("Polymarket Fed Uncertainty: {:.0}%", fed_tension);
        snapshots.push(FeedSnapshot::new("polymarket_fed", "polymarket", fed_tension, &label));

        // BTC bullish score
        let btc_score = macro_sentiment.btc_bullish_probability * 100.0;
        let label = format!("Polymarket BTC Bullish: {:.0}%", btc_score);
        snapshots.push(FeedSnapshot::new("polymarket_btc", "polymarket", btc_score, &label));

        // Overall risk sentiment
        let risk_score = macro_sentiment.safe_haven_demand * 100.0;
        let regime_label = if macro_sentiment.confidence_modifier > 1.2 { "RISK_ON" } else if macro_sentiment.confidence_modifier < 0.8 { "RISK_OFF" } else { "NEUTRAL" };
        snapshots.push(FeedSnapshot::new("polymarket_risk", "polymarket", risk_score, &label));

        // ── AlphaVantage News Sentiment ────────────────────────────────────
        if let Some(av_key) = &self.config.alpha_vantage_api_key {
            let av = AlphaVantageClient::new(av_key.clone());
            if let Some(sentiment) = av.get_sentiment().await {
                // bullish_ratio 0-1 → 0-100
                let score = (sentiment.overall_score + 1.0) / 2.0 * 100.0;
                let label = format!("AV News: sentiment {:.1} ({} articles)", sentiment.overall_score, sentiment.articles_analyzed);
                snapshots.push(FeedSnapshot::new("alphavantage_news", "alphavantage", score, &label));
            }
        }

        // ── NewsAPI ───────────────────────────────────────────────────────
        if let Some(news_key) = &self.config.newsapi_key {
            let newsapi = NewsApiClient::new(news_key.clone());
            if let Some(analysis) = newsapi.get_analysis().await {
                let uncertainty = analysis.regime_event.uncertainty_boost() * 100.0;
                let label = format!("NewsAPI Uncertainty: {:.0}% — Risk boost: {:.2}x", 
                    uncertainty, analysis.regime_event.uncertainty_boost());
                snapshots.push(FeedSnapshot::new("newsapi_uncertainty", "newsapi", uncertainty, &label));
            }
        }

        // ── Finnhub (Economic Events today) ──────────────────────────────
        if let Some(fh_key) = &self.config.finnhub_api_key {
            let fh = FinnhubClient::new(fh_key.clone());
            let now = Utc::now();
            let from = (now - chrono::Duration::hours(24)).date_naive();
            let to = (now + chrono::Duration::hours(24)).date_naive();
            if let Ok(events) = fh.get_economic_calendar(from, to).await {
                let high_impact = events.iter().filter(|e| e.impact == "high").count();
                let score = (high_impact as f64 * 20.0).min(100.0);
                let label = format!("Finnhub Calendar: {} high-impact events in 24h", high_impact);
                snapshots.push(FeedSnapshot::new("finnhub_events", "finnhub", score, &label));
            }
        }

        // Store all snapshots to DB
        let stored = self.store_snapshots(&snapshots).await?;
        info!("🧠 Intelligence: collected {} feed snapshots", stored);

        Ok(snapshots)
    }

    async fn store_snapshots(&self, snapshots: &[FeedSnapshot]) -> Result<usize> {
        let mut count = 0;
        for snap in snapshots {
            let raw = snap.raw_json.as_ref().map(|j| j.to_string());
            sqlx::query(
                "INSERT INTO intelligence_feed_snapshots (feed_id, feed_source, value, label, raw_json, fetched_at)
                 VALUES ($1, $2, $3, $4, $5::jsonb, $6)"
            )
            .bind(&snap.feed_id)
            .bind(&snap.feed_source)
            .bind(snap.value)
            .bind(&snap.label)
            .bind(raw)
            .bind(snap.fetched_at)
            .execute(&self.pool)
            .await?;
            count += 1;
        }
        Ok(count)
    }

    /// Get latest snapshot for each feed (for the frontend feed monitor)
    pub async fn get_latest_snapshots(&self) -> Result<Vec<FeedSnapshot>> {
        let rows = sqlx::query!(
            r#"SELECT DISTINCT ON (feed_id) feed_id, feed_source, value, label, raw_json, fetched_at
               FROM intelligence_feed_snapshots
               ORDER BY feed_id, fetched_at DESC"#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| FeedSnapshot {
            feed_id: r.feed_id,
            feed_source: r.feed_source,
            value: r.value,
            label: r.label.unwrap_or_default(),
            raw_json: r.raw_json,
            fetched_at: r.fetched_at,
        }).collect())
    }
}

fn vix_label(vix: f64) -> &'static str {
    match vix as u32 {
        0..=12 => "Extreme Complacency",
        13..=19 => "Low Volatility",
        20..=29 => "Moderate Volatility",
        30..=39 => "High Fear",
        _ => "Extreme Fear",
    }
}
