//! Signal Tracker — persists auto-eligible Polymarket signals and tracks P&L.
//!
//! Responsibilities:
//!   1. persist_signal()      — INSERT a SignalMatch into polymarket_signals (dedup: 1h window)
//!   2. update_current_prices() — background task: refresh current_price + pnl from market_snapshots
//!   3. get_signal_history()  — paginated SELECT for the history API

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

use super::signal_intelligence::SignalMatch;

// ─── Public return type ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PersistedSignal {
    pub id:               Uuid,
    pub market_id:        String,
    pub condition_id:     String,
    pub market_title:     String,
    pub market_url:       String,
    pub outcome_name:     String,
    pub recommended_side: String,
    pub detection_price:  f64,
    pub current_price:    Option<f64>,
    pub match_score:      f64,
    pub confidence_pct:   f64,
    pub matched_keywords: Vec<String>,
    pub source_count:     i32,
    pub status:           String,
    /// USDC invested (set when executed)
    pub usdc_amount:      Option<f64>,
    pub shares_bought:    Option<f64>,
    pub entry_price:      Option<f64>,
    pub exit_price:       Option<f64>,
    /// Unrealised or realised P&L in USDC
    pub pnl_usdc:         Option<f64>,
    pub order_id:         Option<String>,
    pub detected_at:      DateTime<Utc>,
    pub executed_at:      Option<DateTime<Utc>>,
    pub closed_at:        Option<DateTime<Utc>>,
}

// ─── Tracker ─────────────────────────────────────────────────────────────────

pub struct SignalTracker {
    pool: PgPool,
}

impl SignalTracker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a signal into polymarket_signals.
    /// Dedup: skip if the same market+outcome already has a row in the last hour.
    pub async fn persist_signal(&self, signal: &SignalMatch) {
        // Dedup check
        let already: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM polymarket_signals
            WHERE market_id    = $1
              AND outcome_name = $2
              AND detected_at  > NOW() - INTERVAL '1 hour'
            "#,
        )
        .bind(&signal.market_id)
        .bind(&signal.outcome_name)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(Some(0));

        if already.unwrap_or(0) > 0 {
            return; // already tracked this hour
        }

        let keywords: Vec<String> = signal.matched_keywords.clone();

        let result = sqlx::query(
            r#"
            INSERT INTO polymarket_signals
                (market_id, condition_id, market_title, market_url, outcome_name,
                 recommended_side, detection_price, current_price,
                 match_score, confidence_pct, matched_keywords, source_count,
                 status, detected_at)
            VALUES
                ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'OPEN',NOW())
            "#,
        )
        .bind(&signal.market_id)
        .bind(&signal.condition_id)
        .bind(&signal.market_title)
        .bind(&signal.market_url)
        .bind(&signal.outcome_name)
        .bind(&signal.recommended_side)
        .bind(signal.current_price)
        .bind(signal.current_price)
        .bind(signal.match_score)
        .bind(signal.confidence_pct)
        .bind(&keywords)
        .bind(signal.source_count as i32)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => info!(
                "📌 Signal persisted: {} → {} @ {:.0}¢ (score {:.1}, side {})",
                signal.market_title,
                signal.outcome_name,
                signal.current_price * 100.0,
                signal.match_score,
                signal.recommended_side,
            ),
            Err(e) => warn!("Failed to persist signal for {}: {}", signal.market_id, e),
        }
    }

    /// Pull latest price from market_snapshots for every OPEN signal and
    /// recompute unrealised P&L. Called every 5 minutes by the scheduler.
    pub async fn update_current_prices(&self) {
        use sqlx::Row;

        // Fetch all OPEN signals
        let open = sqlx::query(
            r#"
            SELECT id, market_id, outcome_name, entry_price, usdc_amount
            FROM polymarket_signals
            WHERE status = 'OPEN'
            "#,
        )
        .fetch_all(&self.pool)
        .await;

        let rows = match open {
            Ok(r) => r,
            Err(e) => { warn!("update_current_prices fetch failed: {}", e); return; }
        };

        for row in rows {
            let id: Uuid       = row.try_get("id").unwrap_or_else(|_| Uuid::new_v4());
            let market_id: String = row.try_get("market_id").unwrap_or_default();
            let outcome: String   = row.try_get("outcome_name").unwrap_or_default();
            let entry_price: Option<f64> = row.try_get("entry_price").ok().flatten();
            let usdc_amount: Option<f64> = row.try_get("usdc_amount").ok().flatten();

            // Get latest price from market_snapshots
            let latest_price: Option<f64> = sqlx::query_scalar(
                r#"
                SELECT price FROM market_snapshots
                WHERE market_id = $1 AND outcome_name = $2
                ORDER BY captured_at DESC LIMIT 1
                "#,
            )
            .bind(&market_id)
            .bind(&outcome)
            .fetch_optional(&self.pool)
            .await
            .unwrap_or(None);

            let Some(price) = latest_price else { continue };

            // Compute unrealised P&L if we have an executed position
            let pnl = match (entry_price, usdc_amount) {
                (Some(ep), Some(usdc)) if ep > 0.0 => {
                    let shares = usdc / ep;
                    Some((price - ep) * shares)
                }
                _ => None,
            };

            let _ = sqlx::query(
                r#"
                UPDATE polymarket_signals
                SET current_price = $1, pnl_usdc = $2
                WHERE id = $3
                "#,
            )
            .bind(price)
            .bind(pnl)
            .bind(id)
            .execute(&self.pool)
            .await;
        }
    }

    /// Fetch paginated signal history ordered by detected_at DESC.
    pub async fn get_signal_history(
        &self,
        limit: i64,
    ) -> Result<Vec<PersistedSignal>, anyhow::Error> {
        use sqlx::Row;

        let rows = sqlx::query(
            r#"
            SELECT id, market_id, condition_id, market_title, market_url,
                   outcome_name, recommended_side, detection_price, current_price,
                   match_score, confidence_pct, matched_keywords, source_count,
                   status, usdc_amount, shares_bought, entry_price, exit_price,
                   pnl_usdc, order_id, detected_at, executed_at, closed_at
            FROM polymarket_signals
            ORDER BY detected_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| PersistedSignal {
                id:               r.try_get("id").unwrap_or_else(|_| Uuid::new_v4()),
                market_id:        r.try_get("market_id").unwrap_or_default(),
                condition_id:     r.try_get("condition_id").unwrap_or_default(),
                market_title:     r.try_get("market_title").unwrap_or_default(),
                market_url:       r.try_get("market_url").unwrap_or_default(),
                outcome_name:     r.try_get("outcome_name").unwrap_or_default(),
                recommended_side: r.try_get("recommended_side").unwrap_or_default(),
                detection_price:  r.try_get("detection_price").unwrap_or(0.0),
                current_price:    r.try_get("current_price").ok().flatten(),
                match_score:      r.try_get("match_score").unwrap_or(0.0),
                confidence_pct:   r.try_get("confidence_pct").unwrap_or(0.0),
                matched_keywords: r.try_get("matched_keywords").unwrap_or_default(),
                source_count:     r.try_get("source_count").unwrap_or(0),
                status:           r.try_get("status").unwrap_or_default(),
                usdc_amount:      r.try_get("usdc_amount").ok().flatten(),
                shares_bought:    r.try_get("shares_bought").ok().flatten(),
                entry_price:      r.try_get("entry_price").ok().flatten(),
                exit_price:       r.try_get("exit_price").ok().flatten(),
                pnl_usdc:         r.try_get("pnl_usdc").ok().flatten(),
                order_id:         r.try_get("order_id").ok().flatten(),
                detected_at:      r.try_get("detected_at").unwrap_or_else(|_| Utc::now()),
                executed_at:      r.try_get("executed_at").ok().flatten(),
                closed_at:        r.try_get("closed_at").ok().flatten(),
            })
            .collect())
    }

    /// Manually close a signal with a given exit price and compute final P&L.
    pub async fn close_signal(
        &self,
        id: Uuid,
        exit_price: f64,
    ) -> Result<(), anyhow::Error> {
        use sqlx::Row;

        // Fetch the signal to compute P&L
        let row = sqlx::query(
            "SELECT entry_price, usdc_amount FROM polymarket_signals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let pnl = row.and_then(|r| {
            let ep: Option<f64> = r.try_get("entry_price").ok().flatten();
            let usdc: Option<f64> = r.try_get("usdc_amount").ok().flatten();
            match (ep, usdc) {
                (Some(ep), Some(usdc)) if ep > 0.0 => {
                    let shares = usdc / ep;
                    Some((exit_price - ep) * shares)
                }
                _ => None,
            }
        });

        sqlx::query(
            r#"
            UPDATE polymarket_signals
            SET status      = 'CLOSED',
                exit_price  = $1,
                pnl_usdc    = $2,
                current_price = $1,
                closed_at   = NOW()
            WHERE id = $3
            "#,
        )
        .bind(exit_price)
        .bind(pnl)
        .bind(id)
        .execute(&self.pool)
        .await?;

        info!("🔒 Signal {} closed @ {:.0}¢, P&L: {:?}", id, exit_price * 100.0, pnl);
        Ok(())
    }
}
