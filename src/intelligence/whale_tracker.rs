//! Whale Tracker — monitors large on-chain transactions via Whale Alert API

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

const MIN_USD_VALUE: f64 = 500_000.0; // $500K minimum

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhaleAlert {
    pub id: Option<uuid::Uuid>,
    pub blockchain: String,
    pub symbol: String,
    pub amount_usd: f64,
    pub amount_native: Option<f64>,
    pub from_label: Option<String>,
    pub to_label: Option<String>,
    pub tx_hash: Option<String>,
    pub market_impact: String,   // "BULLISH", "BEARISH", "NEUTRAL"
    pub impact_reason: Option<String>,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct WhaleAlertResponse {
    transactions: Option<Vec<WhaleTransaction>>,
}

#[derive(Debug, Deserialize)]
struct WhaleTransaction {
    id: Option<String>,
    blockchain: String,
    symbol: String,
    amount: f64,
    amount_usd: f64,
    from: Option<WhaleAddress>,
    to: Option<WhaleAddress>,
    transaction_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WhaleAddress {
    address: Option<String>,
    owner: Option<String>,
    owner_type: Option<String>,
}

pub struct WhaleTracker {
    pool: PgPool,
    api_key: Option<String>,
}

impl WhaleTracker {
    pub fn new(pool: PgPool, api_key: Option<String>) -> Self {
        Self { pool, api_key }
    }

    pub async fn run_scheduler(self: Arc<Self>) {
        info!("🐋 Whale tracker starting...");
        let mut ticker = interval(Duration::from_secs(60)); // every 60s
        loop {
            ticker.tick().await;
            if let Err(e) = self.fetch_and_store().await {
                warn!("Whale tracker error: {}", e);
            }
        }
    }

    pub async fn fetch_and_store(&self) -> Result<Vec<WhaleAlert>> {
        let key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                // No API key — return empty (not an error, just not configured)
                return Ok(Vec::new());
            }
        };

        let since = Utc::now().timestamp() - 60;
        let url = format!(
            "https://api.whale-alert.io/v1/transactions?api_key={}&min_value={}&start={}",
            key, MIN_USD_VALUE as u64, since
        );

        let client = reqwest::Client::new();
        let resp = client.get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            // Free tier has limits — don't fail loudly
            return Ok(Vec::new());
        }

        let data: WhaleAlertResponse = resp.json().await?;
        let txs = data.transactions.unwrap_or_default();

        let mut alerts = Vec::new();
        for tx in &txs {
            if tx.amount_usd < MIN_USD_VALUE { continue; }

            let from_label = tx.from.as_ref()
                .and_then(|f| f.owner.clone())
                .or_else(|| tx.from.as_ref().and_then(|f| f.owner_type.clone()));
            let to_label = tx.to.as_ref()
                .and_then(|t| t.owner.clone())
                .or_else(|| tx.to.as_ref().and_then(|t| t.owner_type.clone()));

            let (impact, reason) = classify_impact(&tx.symbol, &from_label, &to_label, tx.amount_usd);

            let alert = WhaleAlert {
                id: None,
                blockchain: tx.blockchain.clone(),
                symbol: tx.symbol.to_uppercase(),
                amount_usd: tx.amount_usd,
                amount_native: Some(tx.amount),
                from_label: from_label.clone(),
                to_label: to_label.clone(),
                tx_hash: tx.id.clone(),
                market_impact: impact,
                impact_reason: reason,
                detected_at: Utc::now(),
            };

            // Avoid duplicates by tx_hash
            if let Some(hash) = &alert.tx_hash {
                let exists = sqlx::query_scalar!(
                    "SELECT COUNT(*) FROM whale_alerts WHERE tx_hash = $1", hash
                )
                .fetch_one(&self.pool)
                .await?
                .unwrap_or(0);

                if exists == 0 {
                    self.store_alert(&alert).await?;
                    alerts.push(alert);
                }
            } else {
                self.store_alert(&alert).await?;
                alerts.push(alert);
            }
        }

        if !alerts.is_empty() {
            info!("🐋 {} new whale transactions detected", alerts.len());
        }

        Ok(alerts)
    }

    async fn store_alert(&self, alert: &WhaleAlert) -> Result<()> {
        sqlx::query(
            "INSERT INTO whale_alerts (blockchain, symbol, amount_usd, amount_native, from_label, to_label, tx_hash, market_impact, impact_reason, detected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(&alert.blockchain)
        .bind(&alert.symbol)
        .bind(alert.amount_usd)
        .bind(alert.amount_native)
        .bind(&alert.from_label)
        .bind(&alert.to_label)
        .bind(&alert.tx_hash)
        .bind(&alert.market_impact)
        .bind(&alert.impact_reason)
        .bind(alert.detected_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_recent_alerts(&self, limit: i64) -> Result<Vec<WhaleAlert>> {
        let rows = sqlx::query!(
            "SELECT id, blockchain, symbol, amount_usd, amount_native, from_label, to_label, tx_hash, market_impact, impact_reason, detected_at
             FROM whale_alerts ORDER BY detected_at DESC LIMIT $1", limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| WhaleAlert {
            id: Some(r.id),
            blockchain: r.blockchain,
            symbol: r.symbol,
            amount_usd: r.amount_usd,
            amount_native: r.amount_native,
            from_label: r.from_label,
            to_label: r.to_label,
            tx_hash: r.tx_hash,
            market_impact: r.market_impact.unwrap_or_default(),
            impact_reason: r.impact_reason,
            detected_at: r.detected_at,
        }).collect())
    }
}

/// Classify likely market impact based on wallet labels
fn classify_impact(symbol: &str, from: &Option<String>, to: &Option<String>, amount_usd: f64) -> (String, Option<String>) {
    let from_str = from.as_deref().unwrap_or("unknown").to_lowercase();
    let to_str = to.as_deref().unwrap_or("unknown").to_lowercase();
    let amount_m = amount_usd / 1_000_000.0;

    // Exchange inflows = potential sell pressure
    let exchange_keywords = ["binance", "coinbase", "kraken", "bybit", "okx", "huobi", "exchange"];
    let to_exchange = exchange_keywords.iter().any(|k| to_str.contains(k));
    let from_exchange = exchange_keywords.iter().any(|k| from_str.contains(k));

    if to_exchange && !from_exchange {
        return (
            "BEARISH".to_string(),
            Some(format!("${:.0}M {} moved to exchange (potential sell) — {} → {}", amount_m, symbol, from_str, to_str))
        );
    }

    // Exchange outflows = potential accumulation
    if from_exchange && !to_exchange {
        return (
            "BULLISH".to_string(),
            Some(format!("${:.0}M {} withdrawn from exchange (accumulation) — {} → {}", amount_m, symbol, from_str, to_str))
        );
    }

    // Wallet to wallet (unknown) = neutral but notable
    (
        "NEUTRAL".to_string(),
        Some(format!("${:.0}M {} transfer — {} → {}", amount_m, symbol, from_str, to_str))
    )
}
