//! Trade Context: Market conditions captured at trade entry for learning/adaptation
//!
//! This module enables performance analysis by condition (RSI level, regime, time, etc.)
//! and provides the data foundation for adaptive parameter adjustment.

use chrono::{DateTime, Utc, Datelike, Timelike};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::mrate::MrateOutput;

/// Trading session based on UTC hour
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingSession {
    Asian,      // 00:00 - 07:00 UTC (Tokyo/Sydney)
    London,     // 07:00 - 12:00 UTC
    Overlap,    // 12:00 - 16:00 UTC (London + NY)
    NewYork,    // 16:00 - 21:00 UTC
    Quiet,      // 21:00 - 00:00 UTC
}

impl TradingSession {
    pub fn from_hour(hour: u32) -> Self {
        match hour {
            0..=6 => TradingSession::Asian,
            7..=11 => TradingSession::London,
            12..=15 => TradingSession::Overlap,
            16..=20 => TradingSession::NewYork,
            _ => TradingSession::Quiet,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            TradingSession::Asian => "ASIAN",
            TradingSession::London => "LONDON",
            TradingSession::Overlap => "OVERLAP",
            TradingSession::NewYork => "NEW_YORK",
            TradingSession::Quiet => "QUIET",
        }
    }
}

/// Market context captured at trade entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeContext {
    pub id: Uuid,
    pub external_trade_id: Option<String>,
    pub bot_id: Uuid,
    pub strategy_id: Option<Uuid>,
    
    // Trade basics
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub lot_size: f64,
    
    // Technical indicators
    pub rsi_at_entry: Option<f64>,
    pub adx_at_entry: Option<f64>,
    pub atr_at_entry: Option<f64>,
    pub ema_trend_at_entry: Option<f64>,
    pub ema_entry_at_entry: Option<f64>,
    pub price_vs_ema_pct: Option<f64>,
    pub volatility_ratio: Option<f64>,
    
    // MRATE context
    pub mrate_regime: Option<String>,
    pub mrate_favored_instrument: Option<String>,
    pub mrate_trend_weight: Option<f64>,
    pub mrate_risk_multiplier: Option<f64>,
    pub mrate_liquidity_score: Option<f64>,
    pub mrate_uncertainty_score: Option<f64>,
    
    // Time context
    pub entry_hour: i32,
    pub entry_day_of_week: i32,
    pub trading_session: String,
    
    // Price action context
    pub recent_move_atr: Option<f64>,
    pub candles_since_last_signal: Option<i32>,
    
    // Signal details
    pub signal_confidence: Option<f64>,
    pub signal_reason: Option<String>,
    
    // Strategy params
    pub strategy_params: Option<serde_json::Value>,
    
    // Outcome (filled later)
    pub exit_price: Option<f64>,
    pub pnl: Option<f64>,
    pub pnl_atr: Option<f64>,
    pub trade_duration_minutes: Option<i32>,
    pub exit_reason: Option<String>,
    pub is_winner: Option<bool>,
    
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Builder for capturing trade context at entry time
#[derive(Debug, Clone, Default)]
pub struct TradeContextBuilder {
    pub external_trade_id: Option<String>,
    pub bot_id: Option<Uuid>,
    pub strategy_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub direction: Option<String>,
    pub entry_price: Option<f64>,
    pub lot_size: Option<f64>,
    
    // Technical
    pub rsi: Option<f64>,
    pub adx: Option<f64>,
    pub atr: Option<f64>,
    pub ema_trend: Option<f64>,
    pub ema_entry: Option<f64>,
    pub volatility_ratio: Option<f64>,
    pub recent_move_atr: Option<f64>,
    
    // MRATE
    pub mrate: Option<MrateOutput>,
    
    // Signal
    pub confidence: Option<f64>,
    pub reason: Option<String>,
    pub strategy_params: Option<serde_json::Value>,
    
    pub candles_since_last_signal: Option<i32>,
}

impl TradeContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn bot_id(mut self, id: Uuid) -> Self {
        self.bot_id = Some(id);
        self
    }
    
    pub fn strategy_id(mut self, id: Uuid) -> Self {
        self.strategy_id = Some(id);
        self
    }
    
    pub fn external_trade_id(mut self, id: String) -> Self {
        self.external_trade_id = Some(id);
        self
    }
    
    pub fn symbol(mut self, s: &str) -> Self {
        self.symbol = Some(s.to_string());
        self
    }
    
    pub fn direction(mut self, d: &str) -> Self {
        self.direction = Some(d.to_string());
        self
    }
    
    pub fn entry_price(mut self, p: f64) -> Self {
        self.entry_price = Some(p);
        self
    }
    
    pub fn lot_size(mut self, l: f64) -> Self {
        self.lot_size = Some(l);
        self
    }
    
    pub fn technical_indicators(mut self, rsi: f64, adx: f64, atr: f64) -> Self {
        self.rsi = Some(rsi);
        self.adx = Some(adx);
        self.atr = Some(atr);
        self
    }
    
    pub fn ema_values(mut self, trend: f64, entry: f64) -> Self {
        self.ema_trend = Some(trend);
        self.ema_entry = Some(entry);
        self
    }
    
    pub fn volatility_ratio(mut self, ratio: f64) -> Self {
        self.volatility_ratio = Some(ratio);
        self
    }
    
    pub fn recent_move_atr(mut self, move_atr: f64) -> Self {
        self.recent_move_atr = Some(move_atr);
        self
    }
    
    pub fn mrate(mut self, mrate: MrateOutput) -> Self {
        self.mrate = Some(mrate);
        self
    }
    
    pub fn signal(mut self, confidence: f64, reason: &str) -> Self {
        self.confidence = Some(confidence);
        self.reason = Some(reason.to_string());
        self
    }
    
    pub fn strategy_params(mut self, params: serde_json::Value) -> Self {
        self.strategy_params = Some(params);
        self
    }
    
    pub fn candles_since_last_signal(mut self, count: i32) -> Self {
        self.candles_since_last_signal = Some(count);
        self
    }
    
    /// Build the TradeContext with current timestamp
    pub fn build(self) -> Result<TradeContext, &'static str> {
        let now = Utc::now();
        let hour = now.hour() as i32;
        let day_of_week = now.weekday().num_days_from_sunday() as i32;
        let session = TradingSession::from_hour(now.hour());
        
        // Calculate price vs EMA percentage
        let price_vs_ema_pct = match (self.entry_price, self.ema_trend) {
            (Some(p), Some(e)) if e > 0.0 => Some((p - e) / e * 100.0),
            _ => None,
        };
        
        // Extract MRATE context
        let (mrate_regime, mrate_favored, mrate_trend_weight, mrate_risk_mult, mrate_liq, mrate_unc) = 
            if let Some(ref m) = self.mrate {
                (
                    Some(m.regime.as_str().to_string()),
                    Some(m.favored_instrument.as_str().to_string()),
                    Some(m.strategy_weights.trend),
                    Some(m.risk_multiplier),
                    Some(m.liquidity_score),
                    Some(m.uncertainty_score),
                )
            } else {
                (None, None, None, None, None, None)
            };
        
        Ok(TradeContext {
            id: Uuid::new_v4(),
            external_trade_id: self.external_trade_id,
            bot_id: self.bot_id.ok_or("bot_id is required")?,
            strategy_id: self.strategy_id,
            symbol: self.symbol.ok_or("symbol is required")?,
            direction: self.direction.ok_or("direction is required")?,
            entry_price: self.entry_price.ok_or("entry_price is required")?,
            lot_size: self.lot_size.ok_or("lot_size is required")?,
            
            rsi_at_entry: self.rsi,
            adx_at_entry: self.adx,
            atr_at_entry: self.atr,
            ema_trend_at_entry: self.ema_trend,
            ema_entry_at_entry: self.ema_entry,
            price_vs_ema_pct,
            volatility_ratio: self.volatility_ratio,
            
            mrate_regime,
            mrate_favored_instrument: mrate_favored,
            mrate_trend_weight,
            mrate_risk_multiplier: mrate_risk_mult,
            mrate_liquidity_score: mrate_liq,
            mrate_uncertainty_score: mrate_unc,
            
            entry_hour: hour,
            entry_day_of_week: day_of_week,
            trading_session: session.as_str().to_string(),
            
            recent_move_atr: self.recent_move_atr,
            candles_since_last_signal: self.candles_since_last_signal,
            
            signal_confidence: self.confidence,
            signal_reason: self.reason,
            strategy_params: self.strategy_params,
            
            // Outcome fields - filled when trade closes
            exit_price: None,
            pnl: None,
            pnl_atr: None,
            trade_duration_minutes: None,
            exit_reason: None,
            is_winner: None,
            
            created_at: now,
            closed_at: None,
        })
    }
}

/// Database row for trade_context
#[derive(Debug, Clone, FromRow)]
pub struct TradeContextRow {
    pub id: Uuid,
    pub external_trade_id: Option<String>,
    pub bot_id: Uuid,
    pub strategy_id: Option<Uuid>,
    pub symbol: String,
    pub direction: String,
    pub entry_price: f64,
    pub lot_size: f64,
    pub rsi_at_entry: Option<f64>,
    pub adx_at_entry: Option<f64>,
    pub atr_at_entry: Option<f64>,
    pub ema_trend_at_entry: Option<f64>,
    pub ema_entry_at_entry: Option<f64>,
    pub price_vs_ema_pct: Option<f64>,
    pub volatility_ratio: Option<f64>,
    pub mrate_regime: Option<String>,
    pub mrate_favored_instrument: Option<String>,
    pub mrate_trend_weight: Option<f64>,
    pub mrate_risk_multiplier: Option<f64>,
    pub mrate_liquidity_score: Option<f64>,
    pub mrate_uncertainty_score: Option<f64>,
    pub entry_hour: i32,
    pub entry_day_of_week: i32,
    pub trading_session: Option<String>,
    pub recent_move_atr: Option<f64>,
    pub candles_since_last_signal: Option<i32>,
    pub signal_confidence: Option<f64>,
    pub signal_reason: Option<String>,
    pub strategy_params: Option<serde_json::Value>,
    pub exit_price: Option<f64>,
    pub pnl: Option<f64>,
    pub pnl_atr: Option<f64>,
    pub trade_duration_minutes: Option<i32>,
    pub exit_reason: Option<String>,
    pub is_winner: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Insert trade context into database
pub async fn insert_trade_context(
    pool: &sqlx::PgPool,
    ctx: &TradeContext,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        INSERT INTO trade_context (
            id, external_trade_id, bot_id, strategy_id,
            symbol, direction, entry_price, lot_size,
            rsi_at_entry, adx_at_entry, atr_at_entry,
            ema_trend_at_entry, ema_entry_at_entry, price_vs_ema_pct, volatility_ratio,
            mrate_regime, mrate_favored_instrument, mrate_trend_weight, mrate_risk_multiplier,
            mrate_liquidity_score, mrate_uncertainty_score,
            entry_hour, entry_day_of_week, trading_session,
            recent_move_atr, candles_since_last_signal,
            signal_confidence, signal_reason, strategy_params,
            created_at
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8,
            $9, $10, $11,
            $12, $13, $14, $15,
            $16, $17, $18, $19,
            $20, $21,
            $22, $23, $24,
            $25, $26,
            $27, $28, $29,
            $30
        )
        RETURNING id
        "#,
    )
    .bind(ctx.id)
    .bind(&ctx.external_trade_id)
    .bind(ctx.bot_id)
    .bind(ctx.strategy_id)
    .bind(&ctx.symbol)
    .bind(&ctx.direction)
    .bind(ctx.entry_price)
    .bind(ctx.lot_size)
    .bind(ctx.rsi_at_entry)
    .bind(ctx.adx_at_entry)
    .bind(ctx.atr_at_entry)
    .bind(ctx.ema_trend_at_entry)
    .bind(ctx.ema_entry_at_entry)
    .bind(ctx.price_vs_ema_pct)
    .bind(ctx.volatility_ratio)
    .bind(&ctx.mrate_regime)
    .bind(&ctx.mrate_favored_instrument)
    .bind(ctx.mrate_trend_weight)
    .bind(ctx.mrate_risk_multiplier)
    .bind(ctx.mrate_liquidity_score)
    .bind(ctx.mrate_uncertainty_score)
    .bind(ctx.entry_hour)
    .bind(ctx.entry_day_of_week)
    .bind(&ctx.trading_session)
    .bind(ctx.recent_move_atr)
    .bind(ctx.candles_since_last_signal)
    .bind(ctx.signal_confidence)
    .bind(&ctx.signal_reason)
    .bind(&ctx.strategy_params)
    .bind(ctx.created_at)
    .fetch_one(pool)
    .await
}

/// Update trade context when trade closes
pub async fn update_trade_outcome(
    pool: &sqlx::PgPool,
    external_trade_id: &str,
    exit_price: f64,
    pnl: f64,
    exit_reason: &str,
) -> Result<(), sqlx::Error> {
    // Get ATR at entry to calculate normalized P&L
    let row: Option<(Option<f64>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT atr_at_entry, created_at FROM trade_context WHERE external_trade_id = $1"
    )
    .bind(external_trade_id)
    .fetch_optional(pool)
    .await?;
    
    let (atr_at_entry, created_at) = match row {
        Some((atr, created)) => (atr, created),
        None => return Ok(()), // Trade context not found, skip
    };
    
    let now = Utc::now();
    let duration_minutes = (now - created_at).num_minutes() as i32;
    let pnl_atr = atr_at_entry.map(|atr| if atr > 0.0 { pnl / atr } else { 0.0 });
    let is_winner = pnl > 0.0;
    
    sqlx::query(
        r#"
        UPDATE trade_context SET
            exit_price = $1,
            pnl = $2,
            pnl_atr = $3,
            trade_duration_minutes = $4,
            exit_reason = $5,
            is_winner = $6,
            closed_at = $7
        WHERE external_trade_id = $8
        "#
    )
    .bind(exit_price)
    .bind(pnl)
    .bind(pnl_atr)
    .bind(duration_minutes)
    .bind(exit_reason)
    .bind(is_winner)
    .bind(now)
    .bind(external_trade_id)
    .execute(pool)
    .await?;
    
    Ok(())
}
