//! Event Detector — detects anomalous spikes in intelligence feeds

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use super::aggregator::{FeedSnapshot, IntelligenceAggregator};

const SPIKE_STD_DEVS: f64 = 2.0; // Flag when value exceeds 24h mean by this many std devs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceEvent {
    pub id: Option<uuid::Uuid>,
    pub event_type: String,        // "SPIKE_HIGH", "SPIKE_LOW", "REGIME_CHANGE", "WHALE_CLUSTER"
    pub feed_id: String,
    pub description: String,
    pub severity: String,          // "LOW", "MEDIUM", "HIGH", "CRITICAL"
    pub value: f64,
    pub baseline_value: f64,
    pub std_dev: f64,
    pub z_score: f64,
    pub affected_instruments: Vec<String>,
    pub detected_at: DateTime<Utc>,
}

pub struct EventDetector {
    pool: PgPool,
    aggregator: Arc<IntelligenceAggregator>,
}

impl EventDetector {
    pub fn new(pool: PgPool, aggregator: Arc<IntelligenceAggregator>) -> Self {
        Self { pool, aggregator }
    }

    pub async fn run_scheduler(self: Arc<Self>) {
        info!("🔔 Event detector starting...");
        let mut ticker = interval(Duration::from_secs(300)); // every 5 minutes
        loop {
            ticker.tick().await;
            if let Err(e) = self.detect_and_store().await {
                warn!("Event detector error: {}", e);
            }
        }
    }

    pub async fn detect_and_store(&self) -> Result<Vec<IntelligenceEvent>> {
        let snapshots = self.aggregator.get_latest_snapshots().await?;
        let mut events = Vec::new();

        for snap in &snapshots {
            // Get 24h history for this feed
            let history = self.get_feed_history_24h(&snap.feed_id).await?;
            if history.len() < 5 { continue; } // Not enough data

            let (mean, std_dev) = calculate_stats(&history);
            if std_dev < 0.001 { continue; } // No variance

            let z_score = (snap.value - mean) / std_dev;

            if z_score.abs() >= SPIKE_STD_DEVS {
                let event = self.build_event(snap, mean, std_dev, z_score);
                self.store_event(&event).await?;
                events.push(event);
            }
        }

        // Detect regime change: if tension score jumped >20pts in one cycle
        if let Ok(regime_events) = self.detect_regime_change().await {
            for e in regime_events {
                self.store_event(&e).await?;
                events.push(e);
            }
        }

        if !events.is_empty() {
            info!("🔔 {} intelligence events detected", events.len());
        }

        Ok(events)
    }

    fn build_event(&self, snap: &FeedSnapshot, mean: f64, std_dev: f64, z_score: f64) -> IntelligenceEvent {
        let event_type = if z_score > 0.0 { "SPIKE_HIGH" } else { "SPIKE_LOW" }.to_string();
        let severity = match z_score.abs() {
            z if z >= 4.0 => "CRITICAL",
            z if z >= 3.0 => "HIGH",
            z if z >= 2.5 => "MEDIUM",
            _ => "LOW",
        }.to_string();

        let direction = if z_score > 0.0 { "surged above" } else { "dropped below" };
        let description = format!(
            "{} {} normal range (value: {:.1}, baseline: {:.1}, z={:.1}σ)",
            snap.feed_id, direction, snap.value, mean, z_score
        );

        let affected = instruments_for_feed(&snap.feed_id);

        IntelligenceEvent {
            id: None,
            event_type,
            feed_id: snap.feed_id.clone(),
            description,
            severity,
            value: snap.value,
            baseline_value: mean,
            std_dev,
            z_score,
            affected_instruments: affected,
            detected_at: Utc::now(),
        }
    }

    async fn detect_regime_change(&self) -> Result<Vec<IntelligenceEvent>> {
        let rows = sqlx::query!(
            r#"WITH ordered AS (
                SELECT feed_id, value, fetched_at,
                       LAG(value) OVER (PARTITION BY feed_id ORDER BY fetched_at) as prev_value
                FROM intelligence_feed_snapshots
                WHERE fetched_at > NOW() - INTERVAL '30 minutes'
                ORDER BY fetched_at DESC
            )
            SELECT feed_id, value, prev_value
            FROM ordered
            WHERE prev_value IS NOT NULL AND ABS(value - prev_value) > 20
            LIMIT 10"#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            let delta = row.value - row.prev_value.unwrap_or(row.value);
            let direction = if delta > 0.0 { "increased" } else { "decreased" };
            events.push(IntelligenceEvent {
                id: None,
                event_type: "REGIME_CHANGE".to_string(),
                feed_id: row.feed_id.clone(),
                description: format!("{} {} by {:.1} points in last 30 min", row.feed_id, direction, delta.abs()),
                severity: if delta.abs() > 30.0 { "HIGH" } else { "MEDIUM" }.to_string(),
                value: row.value,
                baseline_value: row.prev_value.unwrap_or(row.value),
                std_dev: 0.0,
                z_score: 0.0,
                affected_instruments: instruments_for_feed(&row.feed_id),
                detected_at: Utc::now(),
            });
        }
        Ok(events)
    }

    async fn get_feed_history_24h(&self, feed_id: &str) -> Result<Vec<f64>> {
        let rows = sqlx::query_scalar!(
            "SELECT value FROM intelligence_feed_snapshots WHERE feed_id = $1 AND fetched_at > NOW() - INTERVAL '24 hours' ORDER BY fetched_at",
            feed_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn store_event(&self, event: &IntelligenceEvent) -> Result<()> {
        let affected_str = event.affected_instruments.join(",");
        sqlx::query(
            "INSERT INTO intelligence_events (event_type, feed_id, description, severity, value, baseline_value, std_dev, z_score, affected_instruments, detected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(&event.event_type)
        .bind(&event.feed_id)
        .bind(&event.description)
        .bind(&event.severity)
        .bind(event.value)
        .bind(event.baseline_value)
        .bind(event.std_dev)
        .bind(event.z_score)
        .bind(&affected_str)
        .bind(event.detected_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_recent_events(&self, limit: i64) -> Result<Vec<IntelligenceEvent>> {
        let rows = sqlx::query!(
            "SELECT id, event_type, feed_id, description, severity, value, baseline_value, std_dev, z_score, affected_instruments, detected_at
             FROM intelligence_events ORDER BY detected_at DESC LIMIT $1", limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| {
            let affected: Vec<String> = r.affected_instruments
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();

            IntelligenceEvent {
                id: Some(r.id),
                event_type: r.event_type,
                feed_id: r.feed_id.unwrap_or_default(),
                description: r.description,
                severity: r.severity,
                value: r.value.unwrap_or(0.0),
                baseline_value: r.baseline_value.unwrap_or(0.0),
                std_dev: r.std_dev.unwrap_or(0.0),
                z_score: r.z_score.unwrap_or(0.0),
                affected_instruments: affected,
                detected_at: r.detected_at,
            }
        }).collect())
    }
}

fn calculate_stats(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    (mean, variance.sqrt())
}

fn instruments_for_feed(feed_id: &str) -> Vec<String> {
    match feed_id {
        "vix" | "yield_curve_slope" | "real_yield_10y" => {
            vec!["XAU_USD".to_string(), "EUR_USD".to_string(), "USD_JPY".to_string()]
        }
        "dxy" => vec!["EUR_USD".to_string(), "USD_JPY".to_string(), "XAU_USD".to_string()],
        "gld_flow" | "fred_tips" => vec!["XAU_USD".to_string()],
        "fear_greed_crypto" | "polymarket_btc" => {
            vec!["BTC_USD".to_string()]
        }
        "polymarket_risk" | "fear_greed_stocks" => {
            vec!["XAU_USD".to_string(), "EUR_USD".to_string(), "BTC_USD".to_string()]
        }
        "reddit_sentiment" | "alphavantage_news" | "newsapi_uncertainty" | "finnhub_events" => {
            vec!["XAU_USD".to_string(), "EUR_USD".to_string(), "BTC_USD".to_string(), "NATGAS_USD".to_string()]
        }
        _ => vec!["XAU_USD".to_string(), "EUR_USD".to_string()],
    }
}
