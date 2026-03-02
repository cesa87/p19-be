//! Polymarket Whale Tracker
//!
//! Tracks large trades from known high-win-rate wallets and cross-references
//! them against active polymarket_signals. When a whale trades in the same
//! direction as a signal, that signal gets a confidence boost.
//!
//! Data source: https://data-api.polymarket.com/activity?user={address}
//!              https://data-api.polymarket.com/trades?conditionId={id}

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

// ─── Polymarket API types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawActivity {
    #[serde(rename = "proxyWallet")]
    proxy_wallet: String,
    #[serde(rename = "conditionId")]
    condition_id: Option<String>,
    side:         Option<String>,
    #[serde(rename = "usdcSize")]
    usdc_size:    Option<f64>,
    size:         Option<f64>,
    price:        Option<f64>,
    timestamp:    Option<i64>,
    title:        Option<String>,
    outcome:      Option<String>,
    #[serde(rename = "transactionHash")]
    transaction_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawTrade {
    #[serde(rename = "proxyWallet")]
    proxy_wallet: String,
    #[serde(rename = "conditionId")]
    condition_id: Option<String>,
    side:         Option<String>,
    size:         Option<f64>,
    price:        Option<f64>,
    timestamp:    Option<i64>,
    title:        Option<String>,
    outcome:      Option<String>,
    #[serde(rename = "transactionHash")]
    transaction_hash: Option<String>,
}

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WhaleTrade {
    pub id:               Uuid,
    pub whale_address:    String,
    pub condition_id:     String,
    pub market_title:     String,
    pub outcome_name:     String,
    pub side:             String,
    pub usdc_size:        f64,
    pub price:            f64,
    pub traded_at:        DateTime<Utc>,
    pub transaction_hash: String,
    pub signal_id:        Option<Uuid>,
    pub is_corroboration: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackedWallet {
    pub address:          String,
    pub alias:            String,
    pub win_rate_pct:     f64,
    pub total_pnl_usd:    f64,
    pub account_age_days: i32,
    pub is_active:        bool,
}

// ─── Fetcher ─────────────────────────────────────────────────────────────────

pub struct WhaleFetcher {
    pool:   PgPool,
    client: Client,
}

impl WhaleFetcher {
    pub fn new(pool: PgPool) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("Mozilla/5.0 Aureum Intelligence Terminal")
            .build()
            .expect("Failed to build reqwest client");
        Self { pool, client }
    }

    /// Main scheduler entry point — polls all active tracked wallets.
    pub async fn poll_all(&self) {
        use sqlx::Row;

        let wallets = sqlx::query(
            "SELECT address, win_rate_pct FROM tracked_whale_wallets WHERE is_active = TRUE AND address NOT LIKE 'PLACEHOLDER%'"
        )
        .fetch_all(&self.pool)
        .await;

        let wallets = match wallets {
            Ok(w) => w,
            Err(e) => { warn!("Whale wallet query failed: {}", e); return; }
        };

        if wallets.is_empty() {
            return;
        }

        info!("🐋 Polling {} tracked whale wallets", wallets.len());

        for row in wallets {
            let address: String  = row.try_get("address").unwrap_or_default();
            let win_rate: f64    = row.try_get("win_rate_pct").unwrap_or(0.0);
            self.poll_wallet(&address, win_rate).await;

            // Update last_polled_at
            let _ = sqlx::query(
                "UPDATE tracked_whale_wallets SET last_polled_at = NOW() WHERE address = $1"
            )
            .bind(&address)
            .execute(&self.pool)
            .await;
        }
    }

    /// Fetch recent activity for a single wallet and store new large trades.
    async fn poll_wallet(&self, address: &str, win_rate: f64) {
        let url = format!(
            "https://data-api.polymarket.com/activity?user={}&limit=50",
            address
        );

        let resp = self.client.get(&url).send().await;
        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => { warn!("Whale activity {} returned {}", &address[..10], r.status()); return; }
            Err(e) => { warn!("Whale activity request failed for {}: {}", &address[..10], e); return; }
        };

        let activities: Vec<RawActivity> = match resp.json().await {
            Ok(a) => a,
            Err(e) => { warn!("Failed to parse activity for {}: {}", &address[..10], e); return; }
        };

        // Only care about TRADE type activity (not REDEEM, etc.) with meaningful size
        for act in activities {
            let usdc = act.usdc_size
                .or_else(|| act.size.zip(act.price).map(|(s, p)| s * p))
                .unwrap_or(0.0);

            // Minimum $500 USDC to qualify as a notable trade
            if usdc < 500.0 { continue; }

            let Some(condition_id) = act.condition_id else { continue };
            let Some(side)         = act.side else { continue };
            let Some(tx_hash)      = act.transaction_hash else { continue };
            let price              = act.price.unwrap_or(0.0);
            let ts                 = act.timestamp.unwrap_or(0);
            let traded_at: DateTime<Utc> = DateTime::from_timestamp(ts, 0)
                .unwrap_or_else(Utc::now);

            self.store_trade(
                &act.proxy_wallet,
                &condition_id,
                act.title.as_deref().unwrap_or(""),
                act.outcome.as_deref().unwrap_or(""),
                &side,
                usdc,
                price,
                traded_at,
                &tx_hash,
                win_rate,
            ).await;
        }
    }

    /// Poll trades for a specific conditionId (for markets where we have active signals).
    /// Stores any trade ≥ $2K regardless of whether the wallet is tracked.
    pub async fn poll_market_trades(&self, condition_id: &str) {
        let url = format!(
            "https://data-api.polymarket.com/trades?conditionId={}&limit=100",
            condition_id
        );

        let resp = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return,
        };

        let trades: Vec<RawTrade> = match resp.json().await {
            Ok(t) => t,
            Err(_) => return,
        };

        for trade in trades {
            let usdc = trade.size.zip(trade.price).map(|(s, p)| s * p).unwrap_or(0.0);
            if usdc < 2000.0 { continue; }

            let Some(cid)     = trade.condition_id else { continue };
            let Some(side)    = trade.side else { continue };
            let Some(tx_hash) = trade.transaction_hash else { continue };
            let price         = trade.price.unwrap_or(0.0);
            let ts            = trade.timestamp.unwrap_or(0);
            let traded_at     = DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now);

            self.store_trade(
                &trade.proxy_wallet,
                &cid,
                trade.title.as_deref().unwrap_or(""),
                trade.outcome.as_deref().unwrap_or(""),
                &side,
                usdc,
                price,
                traded_at,
                &tx_hash,
                0.0, // unknown win rate for untracked wallets
            ).await;
        }
    }

    /// Insert a whale trade, check for signal corroboration, apply confidence boost.
    async fn store_trade(
        &self,
        wallet:      &str,
        condition_id: &str,
        title:       &str,
        outcome:     &str,
        side:        &str,
        usdc_size:   f64,
        price:       f64,
        traded_at:   DateTime<Utc>,
        tx_hash:     &str,
        win_rate:    f64,
    ) {
        // Check for matching OPEN signal
        use sqlx::Row;

        let signal_row = sqlx::query(
            r#"
            SELECT id, recommended_side, whale_corroboration_count, confidence_pct
            FROM polymarket_signals
            WHERE condition_id = $1
              AND status = 'OPEN'
            ORDER BY detected_at DESC
            LIMIT 1
            "#,
        )
        .bind(condition_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        let (signal_id, is_corroboration) = match &signal_row {
            Some(r) => {
                let sig_id: Uuid   = r.try_get("id").unwrap_or_else(|_| Uuid::new_v4());
                let rec_side: String = r.try_get("recommended_side").unwrap_or_default();
                // Corroboration: whale's outcome matches signal's recommended side
                // "Yes"/"YES" → YES, "No"/"NO" → NO
                let whale_side = if outcome.to_lowercase().contains("yes") || side == "BUY" {
                    "YES"
                } else {
                    "NO"
                };
                let corroborated = whale_side == rec_side.as_str();
                (Some(sig_id), corroborated)
            }
            None => (None, false),
        };

        // Insert trade (ignore if tx_hash already exists)
        let result = sqlx::query(
            r#"
            INSERT INTO whale_trades
                (whale_address, condition_id, market_title, outcome_name, side,
                 usdc_size, price, traded_at, transaction_hash, signal_id, is_corroboration)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            ON CONFLICT (transaction_hash) DO NOTHING
            "#,
        )
        .bind(wallet)
        .bind(condition_id)
        .bind(title)
        .bind(outcome)
        .bind(side)
        .bind(usdc_size)
        .bind(price)
        .bind(traded_at)
        .bind(tx_hash)
        .bind(signal_id)
        .bind(is_corroboration)
        .execute(&self.pool)
        .await;

        let rows_affected = result.map(|r| r.rows_affected()).unwrap_or(0);
        if rows_affected == 0 {
            return; // already stored
        }

        if is_corroboration {
            let sig_id = signal_id.unwrap();
            let cur_count: i64 = signal_row.as_ref()
                .and_then(|r| r.try_get("whale_corroboration_count").ok())
                .unwrap_or(0);
            let cur_conf: f64 = signal_row.as_ref()
                .and_then(|r| r.try_get("confidence_pct").ok())
                .unwrap_or(0.0);

            // Boost proportional to win_rate: high WR whale = bigger boost
            let boost = if win_rate >= 70.0 { 20.0 } else if win_rate >= 50.0 { 12.0 } else { 6.0 };
            let new_conf = (cur_conf + boost).min(97.0);

            let _ = sqlx::query(
                r#"
                UPDATE polymarket_signals
                SET whale_corroboration_count = $1,
                    whale_boost_pct           = whale_boost_pct + $2,
                    confidence_pct            = $3
                WHERE id = $4
                "#,
            )
            .bind(cur_count + 1)
            .bind(boost)
            .bind(new_conf)
            .bind(sig_id)
            .execute(&self.pool)
            .await;

            info!(
                "🐋 CORROBORATION: {} ${:.0} {} on '{}' → signal confidence now {:.1}%",
                &wallet[..14], usdc_size, side, &title[..title.len().min(45)], new_conf
            );
        } else {
            info!(
                "🐋 Whale trade stored: {} ${:.0} {} '{}' ({})",
                &wallet[..14], usdc_size, side, &title[..title.len().min(40)], outcome
            );
        }
    }

    // ─── DB helpers ──────────────────────────────────────────────────────────

    pub async fn get_wallets(&self) -> Result<Vec<TrackedWallet>, anyhow::Error> {
        use sqlx::Row;
        let rows = sqlx::query(
            "SELECT address, alias, win_rate_pct, total_pnl_usd, account_age_days, is_active FROM tracked_whale_wallets ORDER BY win_rate_pct DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| TrackedWallet {
            address:          r.try_get("address").unwrap_or_default(),
            alias:            r.try_get("alias").unwrap_or_default(),
            win_rate_pct:     r.try_get("win_rate_pct").unwrap_or(0.0),
            total_pnl_usd:    r.try_get("total_pnl_usd").unwrap_or(0.0),
            account_age_days: r.try_get("account_age_days").unwrap_or(0),
            is_active:        r.try_get("is_active").unwrap_or(true),
        }).collect())
    }

    pub async fn add_wallet(
        &self,
        address: &str,
        alias: &str,
        win_rate_pct: f64,
        total_pnl_usd: f64,
        account_age_days: i32,
    ) -> Result<(), anyhow::Error> {
        sqlx::query(
            r#"
            INSERT INTO tracked_whale_wallets (address, alias, win_rate_pct, total_pnl_usd, account_age_days)
            VALUES ($1,$2,$3,$4,$5)
            ON CONFLICT (address) DO UPDATE
                SET alias = $2, win_rate_pct = $3, total_pnl_usd = $4, account_age_days = $5, is_active = TRUE
            "#,
        )
        .bind(address)
        .bind(alias)
        .bind(win_rate_pct)
        .bind(total_pnl_usd)
        .bind(account_age_days)
        .execute(&self.pool)
        .await?;
        info!("🐋 Whale wallet added: {} ({})", alias, address);
        Ok(())
    }

    pub async fn remove_wallet(&self, address: &str) -> Result<(), anyhow::Error> {
        sqlx::query(
            "UPDATE tracked_whale_wallets SET is_active = FALSE WHERE address = $1"
        )
        .bind(address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_recent_trades(
        &self,
        limit: i64,
        corroborations_only: bool,
    ) -> Result<Vec<WhaleTrade>, anyhow::Error> {
        use sqlx::Row;
        let filter = if corroborations_only { "AND is_corroboration = TRUE" } else { "" };
        let sql = format!(
            r#"
            SELECT id, whale_address, condition_id, market_title, outcome_name,
                   side, usdc_size, price, traded_at, transaction_hash, signal_id, is_corroboration
            FROM whale_trades
            WHERE TRUE {filter}
            ORDER BY traded_at DESC
            LIMIT $1
            "#,
            filter = filter,
        );
        let rows = sqlx::query(&sql).bind(limit).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| WhaleTrade {
            id:               r.try_get("id").unwrap_or_else(|_| Uuid::new_v4()),
            whale_address:    r.try_get("whale_address").unwrap_or_default(),
            condition_id:     r.try_get("condition_id").unwrap_or_default(),
            market_title:     r.try_get("market_title").unwrap_or_default(),
            outcome_name:     r.try_get("outcome_name").unwrap_or_default(),
            side:             r.try_get("side").unwrap_or_default(),
            usdc_size:        r.try_get("usdc_size").unwrap_or(0.0),
            price:            r.try_get("price").unwrap_or(0.0),
            traded_at:        r.try_get("traded_at").unwrap_or_else(|_| Utc::now()),
            transaction_hash: r.try_get("transaction_hash").unwrap_or_default(),
            signal_id:        r.try_get("signal_id").ok().flatten(),
            is_corroboration: r.try_get("is_corroboration").unwrap_or(false),
        }).collect())
    }
}
