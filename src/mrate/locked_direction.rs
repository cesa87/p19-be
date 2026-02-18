//! Locked Direction Management
//! 
//! Institutional approach: Direction is locked once per day at NY close (10pm GMT / 5pm EST)
//! Based on D1 close price vs 200 EMA.
//! 
//! This prevents intraday whipsaw from price temporarily crossing the EMA.

use chrono::{DateTime, Utc, Timelike, Duration, Datelike};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn, error};

use super::models::TradingDirection;

/// Locked direction record from database
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LockedDirection {
    pub instrument: String,
    pub direction: String,
    pub d1_close_price: f64,
    pub ema_200: f64,
    pub locked_at: DateTime<Utc>,
    pub next_update_at: DateTime<Utc>,
}

impl LockedDirection {
    /// Convert direction string to TradingDirection enum
    pub fn trading_direction(&self) -> TradingDirection {
        match self.direction.as_str() {
            "LONG" => TradingDirection::Long,
            "SHORT" => TradingDirection::Short,
            _ => TradingDirection::Neutral,
        }
    }
}

/// Get locked direction for an instrument
pub async fn get_locked_direction(
    pool: &PgPool,
    instrument: &str,
) -> Result<Option<LockedDirection>, sqlx::Error> {
    sqlx::query_as::<_, LockedDirection>(
        "SELECT instrument, direction, d1_close_price::float8, ema_200::float8, locked_at, next_update_at
         FROM locked_directions
         WHERE instrument = $1"
    )
    .bind(instrument)
    .fetch_optional(pool)
    .await
}

/// Get all locked directions
pub async fn get_all_locked_directions(
    pool: &PgPool,
) -> Result<Vec<LockedDirection>, sqlx::Error> {
    sqlx::query_as::<_, LockedDirection>(
        "SELECT instrument, direction, d1_close_price::float8, ema_200::float8, locked_at, next_update_at
         FROM locked_directions
         ORDER BY instrument"
    )
    .fetch_all(pool)
    .await
}

/// Update locked direction for an instrument
pub async fn update_locked_direction(
    pool: &PgPool,
    instrument: &str,
    direction: TradingDirection,
    d1_close_price: f64,
    ema_200: f64,
) -> Result<(), sqlx::Error> {
    let direction_str = match direction {
        TradingDirection::Long => "LONG",
        TradingDirection::Short => "SHORT",
        TradingDirection::Neutral => "NEUTRAL",
    };
    
    let next_update = next_ny_close();
    
    sqlx::query(
        "INSERT INTO locked_directions (instrument, direction, d1_close_price, ema_200, locked_at, next_update_at, updated_at)
         VALUES ($1, $2, $3, $4, NOW(), $5, NOW())
         ON CONFLICT (instrument) DO UPDATE SET
            direction = $2,
            d1_close_price = $3,
            ema_200 = $4,
            locked_at = NOW(),
            next_update_at = $5,
            updated_at = NOW()"
    )
    .bind(instrument)
    .bind(direction_str)
    .bind(d1_close_price)
    .bind(ema_200)
    .bind(next_update)
    .execute(pool)
    .await?;
    
    info!(
        "🔒 Locked direction updated: {} = {} (close: {:.2}, EMA200: {:.2})",
        instrument, direction_str, d1_close_price, ema_200
    );
    
    Ok(())
}

/// Calculate next NY close time (10pm GMT / 5pm EST)
/// D1 candles close at this time in OANDA
pub fn next_ny_close() -> DateTime<Utc> {
    let now = Utc::now();
    let today_close = now.date_naive()
        .and_hms_opt(22, 0, 0)  // 10pm GMT = 5pm EST
        .unwrap();
    let today_close_utc = DateTime::<Utc>::from_naive_utc_and_offset(today_close, Utc);
    
    if now < today_close_utc {
        // Today's close hasn't happened yet
        today_close_utc
    } else {
        // Today's close already passed, next one is tomorrow
        // Skip weekends (Sat/Sun don't have closes)
        let mut next = today_close_utc + Duration::days(1);
        while next.weekday().num_days_from_monday() >= 5 {
            next = next + Duration::days(1);
        }
        next
    }
}

/// Check if we're within the NY close update window (10pm-10:05pm GMT)
pub fn is_ny_close_window() -> bool {
    let now = Utc::now();
    let hour = now.hour();
    let minute = now.minute();
    
    // 10pm GMT window (22:00 - 22:05)
    hour == 22 && minute < 5
}

/// Check if direction needs updating (past next_update_at)
pub fn needs_update(locked: &LockedDirection) -> bool {
    Utc::now() >= locked.next_update_at
}

/// Update all instrument directions at NY close
/// Call this from the MRATE scheduler when in the NY close window
pub async fn update_all_directions_at_ny_close(
    pool: &PgPool,
    oanda: &crate::broker::OandaClient,
) -> Result<usize, String> {
    let instruments = vec![
        "XAU_USD",
        "BTC_USD", 
        "EUR_USD",
        "USD_JPY",
        "WTICO_USD",
        "NATGAS_USD",
    ];
    
    let mut updated = 0;
    
    for instrument in instruments {
        match update_instrument_direction(pool, oanda, instrument).await {
            Ok(true) => updated += 1,
            Ok(false) => {} // No update needed
            Err(e) => {
                error!("Failed to update direction for {}: {}", instrument, e);
            }
        }
    }
    
    if updated > 0 {
        info!("🔒 Updated {} instrument directions at NY close", updated);
    }
    
    Ok(updated)
}

/// Update direction for a single instrument
async fn update_instrument_direction(
    pool: &PgPool,
    oanda: &crate::broker::OandaClient,
    instrument: &str,
) -> Result<bool, String> {
    // Fetch D1 candles (need 201 for 200 EMA + latest close)
    let candles = oanda.get_candles(instrument, "D", 250).await
        .map_err(|e| format!("Failed to fetch candles: {}", e))?;
    
    if candles.len() < 201 {
        warn!("{}: Not enough D1 candles for 200 EMA (got {})", instrument, candles.len());
        return Ok(false);
    }
    
    // Calculate 200 EMA
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let ema_200 = calculate_ema(&closes, 200);
    
    // Get yesterday's close (second-to-last candle, as last might be incomplete)
    let d1_close = candles[candles.len() - 2].close;
    
    // Determine direction
    let direction = if d1_close > ema_200 {
        TradingDirection::Long
    } else if d1_close < ema_200 {
        TradingDirection::Short
    } else {
        TradingDirection::Neutral
    };
    
    // Update database
    update_locked_direction(pool, instrument, direction, d1_close, ema_200).await
        .map_err(|e| format!("Failed to update locked direction: {}", e))?;
    
    Ok(true)
}

/// Calculate EMA for a series
fn calculate_ema(prices: &[f64], period: usize) -> f64 {
    if prices.len() < period {
        return prices.last().copied().unwrap_or(0.0);
    }
    
    let multiplier = 2.0 / (period as f64 + 1.0);
    
    // Start with SMA for first `period` values
    let sma: f64 = prices.iter().take(period).sum::<f64>() / period as f64;
    
    // Calculate EMA from there
    let mut ema = sma;
    for price in prices.iter().skip(period) {
        ema = (price - ema) * multiplier + ema;
    }
    
    ema
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_next_ny_close() {
        let next = next_ny_close();
        assert_eq!(next.hour(), 22);
        assert_eq!(next.minute(), 0);
    }
    
    #[test]
    fn test_ema_calculation() {
        let prices: Vec<f64> = (1..=210).map(|x| x as f64).collect();
        let ema = calculate_ema(&prices, 200);
        // EMA should be close to recent values, slightly lagging
        assert!(ema > 100.0 && ema < 210.0);
    }
}
