//! Arb Scanner — monitors prediction markets for YES+NO spread opportunities

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use crate::macro_sentiment::polymarket::PolymarketClient;

const ARB_THRESHOLD: f64 = 0.995; // Flag when YES+NO < this

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbOpportunity {
    pub id: Option<uuid::Uuid>,
    pub source: String,
    pub market_id: String,
    pub market_name: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub spread_pct: f64,              // (1 - yes - no) * 100
    pub estimated_profit_per_100: f64, // $ profit per $100 deployed
    pub detected_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub status: String,
}

impl ArbOpportunity {
    pub fn new(source: &str, market_id: &str, market_name: &str, yes: f64, no: f64) -> Self {
        let spread = (1.0 - yes - no).max(0.0);
        let profit_per_100 = spread * 100.0;
        Self {
            id: None,
            source: source.to_string(),
            market_id: market_id.to_string(),
            market_name: market_name.to_string(),
            yes_price: yes,
            no_price: no,
            spread_pct: spread * 100.0,
            estimated_profit_per_100: profit_per_100,
            detected_at: Utc::now(),
            closed_at: None,
            status: "OPEN".to_string(),
        }
    }
}

pub struct ArbScanner {
    pool: PgPool,
}

impl ArbScanner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_scheduler(self: Arc<Self>) {
        info!("🎯 Arb scanner starting...");
        let mut ticker = interval(Duration::from_secs(30)); // every 30s
        loop {
            ticker.tick().await;
            if let Err(e) = self.scan_all_markets().await {
                warn!("Arb scanner error: {}", e);
            }
        }
    }

    pub async fn scan_all_markets(&self) -> Result<Vec<ArbOpportunity>> {
        let mut opportunities = Vec::new();

        // ── Polymarket ────────────────────────────────────────────────────
        match self.scan_polymarket().await {
            Ok(mut opps) => opportunities.append(&mut opps),
            Err(e) => warn!("Polymarket scan error: {}", e),
        }

        // Close stale opportunities (no longer open)
        self.close_stale_opportunities(&opportunities).await?;

        if !opportunities.is_empty() {
            info!("🎯 Arb scanner: {} opportunities found", opportunities.len());
        }

        Ok(opportunities)
    }

    async fn scan_polymarket(&self) -> Result<Vec<ArbOpportunity>> {
        let client = PolymarketClient::new();
        let markets = client.fetch_all_markets().await
            .map_err(|e| anyhow::anyhow!(e))?;

        let mut opportunities = Vec::new();

        for market in &markets {
            // Need exactly 2 outcomes (binary market)
            if market.outcomes.len() != 2 || market.outcome_prices.len() != 2 {
                continue;
            }

            let yes_price = market.outcome_prices[0];
            let no_price = market.outcome_prices[1];

            // Skip obviously broken prices
            if yes_price <= 0.0 || no_price <= 0.0 || yes_price >= 1.0 || no_price >= 1.0 {
                continue;
            }

            if yes_price + no_price < ARB_THRESHOLD {
                let opp = ArbOpportunity::new(
                    "polymarket",
                    &market.condition_id,
                    &market.question,
                    yes_price,
                    no_price,
                );

                // Only store if not already tracked as open
                if !self.is_already_tracked(&opp.market_id, "polymarket").await? {
                    self.store_opportunity(&opp).await?;
                }

                opportunities.push(opp);
            }
        }

        Ok(opportunities)
    }

    async fn is_already_tracked(&self, market_id: &str, source: &str) -> Result<bool> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM arb_opportunities WHERE market_id = $1 AND source = $2 AND status = 'OPEN'",
            market_id, source
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(count > 0)
    }

    async fn store_opportunity(&self, opp: &ArbOpportunity) -> Result<()> {
        sqlx::query(
            "INSERT INTO arb_opportunities (source, market_id, market_name, yes_price, no_price, spread_pct, estimated_profit_per_100, detected_at, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'OPEN')"
        )
        .bind(&opp.source)
        .bind(&opp.market_id)
        .bind(&opp.market_name)
        .bind(opp.yes_price)
        .bind(opp.no_price)
        .bind(opp.spread_pct)
        .bind(opp.estimated_profit_per_100)
        .bind(opp.detected_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark opportunities as CLOSED if they no longer appear in current scan
    async fn close_stale_opportunities(&self, current: &[ArbOpportunity]) -> Result<()> {
        let current_ids: Vec<String> = current.iter().map(|o| o.market_id.clone()).collect();
        if current_ids.is_empty() {
            // Close all open ones
            sqlx::query(
                "UPDATE arb_opportunities SET status = 'CLOSED', closed_at = NOW() WHERE status = 'OPEN'"
            )
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE arb_opportunities SET status = 'CLOSED', closed_at = NOW()
                 WHERE status = 'OPEN' AND market_id != ALL($1)"
            )
            .bind(&current_ids)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_live_opportunities(&self) -> Result<Vec<ArbOpportunity>> {
        let rows = sqlx::query!(
            "SELECT id, source, market_id, market_name, yes_price, no_price, spread_pct, estimated_profit_per_100, detected_at, closed_at, status
             FROM arb_opportunities WHERE status = 'OPEN'
             ORDER BY spread_pct DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| ArbOpportunity {
            id: Some(r.id),
            source: r.source,
            market_id: r.market_id,
            market_name: r.market_name,
            yes_price: r.yes_price,
            no_price: r.no_price,
            spread_pct: r.spread_pct,
            estimated_profit_per_100: r.estimated_profit_per_100.unwrap_or(0.0),
            detected_at: r.detected_at,
            closed_at: r.closed_at,
            status: r.status,
        }).collect())
    }

    pub async fn get_opportunity_history(&self, limit: i64) -> Result<Vec<ArbOpportunity>> {
        let rows = sqlx::query!(
            "SELECT id, source, market_id, market_name, yes_price, no_price, spread_pct, estimated_profit_per_100, detected_at, closed_at, status
             FROM arb_opportunities
             ORDER BY detected_at DESC LIMIT $1",
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| ArbOpportunity {
            id: Some(r.id),
            source: r.source,
            market_id: r.market_id,
            market_name: r.market_name,
            yes_price: r.yes_price,
            no_price: r.no_price,
            spread_pct: r.spread_pct,
            estimated_profit_per_100: r.estimated_profit_per_100.unwrap_or(0.0),
            detected_at: r.detected_at,
            closed_at: r.closed_at,
            status: r.status,
        }).collect())
    }
}
