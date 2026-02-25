use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use crate::error::AppError;
use crate::models::Candle;

const OANDA_PRACTICE_URL: &str = "https://api-fxpractice.oanda.com";
const OANDA_LIVE_URL: &str = "https://api-fxtrade.oanda.com";

#[derive(Clone)]
pub struct OandaClient {
    client: reqwest::Client,
    base_url: String,
    account_id: String,
}

impl OandaClient {
    pub fn new(api_token: &str, account_id: &str, is_practice: bool) -> Result<Self, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_token))
                .map_err(|e| AppError::InternalError(e.to_string()))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        let base_url = if is_practice {
            OANDA_PRACTICE_URL.to_string()
        } else {
            OANDA_LIVE_URL.to_string()
        };

        Ok(Self {
            client,
            base_url,
            account_id: account_id.to_string(),
        })
    }

    /// Get account summary
    pub async fn get_account_summary(&self) -> Result<AccountSummary, AppError> {
        let url = format!("{}/v3/accounts/{}/summary", self.base_url, self.account_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: AccountSummaryResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        Ok(data.account)
    }

    /// Get historical candles for an instrument
    pub async fn get_candles(
        &self,
        instrument: &str,
        granularity: &str,
        count: u32,
    ) -> Result<Vec<Candle>, AppError> {
        let url = format!(
            "{}/v3/instruments/{}/candles?granularity={}&count={}",
            self.base_url, instrument, granularity, count
        );

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: CandlesResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        let candles = data.candles
            .into_iter()
            .filter(|c| c.complete)
            .map(|c| Candle {
                time: parse_oanda_time(&c.time),
                open: c.mid.o.parse().unwrap_or(0.0),
                high: c.mid.h.parse().unwrap_or(0.0),
                low: c.mid.l.parse().unwrap_or(0.0),
                close: c.mid.c.parse().unwrap_or(0.0),
                volume: Some(c.volume as f64),
            })
            .collect();

        Ok(candles)
    }

    /// Get historical candles for an instrument by date range
    /// Handles chunking for large date ranges (OANDA limits to ~5000 candles per request)
    pub async fn get_candles_by_date(
        &self,
        instrument: &str,
        granularity: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<Candle>, AppError> {
        use chrono::{DateTime, Duration, Utc};
        
        // Parse the from and to dates
        let from_dt = DateTime::parse_from_rfc3339(from)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| AppError::BadRequest(format!("Invalid from date: {}", e)))?;
        let to_dt = DateTime::parse_from_rfc3339(to)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| AppError::BadRequest(format!("Invalid to date: {}", e)))?;
        
        // Calculate chunk size based on granularity
        // OANDA max is 5000 candles, use 4000 to be safe
        let chunk_duration = match granularity {
            "M1" => Duration::days(2),      // ~2880 candles per 2 days
            "M5" => Duration::days(10),     // ~2880 candles per 10 days
            "M15" => Duration::days(30),    // ~2880 candles per 30 days
            "M30" => Duration::days(60),    // ~2880 candles per 60 days
            "H1" => Duration::days(120),    // ~2880 candles per 120 days
            "H4" => Duration::days(400),    // ~2400 candles per 400 days
            "D" => Duration::days(3000),    // ~3000 candles
            _ => Duration::days(30),        // Default to 30 days
        };
        
        let mut all_candles: Vec<Candle> = Vec::new();
        let mut chunk_start = from_dt;
        let max_iterations = 20;
        
        for iteration in 0..max_iterations {
            if chunk_start >= to_dt {
                break;
            }
            
            let chunk_end = (chunk_start + chunk_duration).min(to_dt);
            
            let from_str = chunk_start.to_rfc3339();
            let to_str = chunk_end.to_rfc3339();
            let from_encoded = urlencoding::encode(&from_str);
            let to_encoded = urlencoding::encode(&to_str);
            
            let url = format!(
                "{}/v3/instruments/{}/candles?granularity={}&from={}&to={}",
                self.base_url, instrument, granularity, from_encoded, to_encoded
            );

            tracing::info!("Fetching candles chunk {} ({} to {})", iteration + 1, from_str, to_str);

            let response = self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

            let status = response.status();
            
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_default();
                tracing::error!("OANDA API error ({}): {}", status, error_text);
                return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
            }

            let data: CandlesResponse = response
                .json()
                .await
                .map_err(|e| {
                    tracing::error!("Failed to parse OANDA candles response: {}", e);
                    AppError::InternalError(format!("Failed to parse response: {}", e))
                })?;

            let batch_candles: Vec<Candle> = data.candles
                .into_iter()
                .filter(|c| c.complete)
                .map(|c| Candle {
                    time: parse_oanda_time(&c.time),
                    open: c.mid.o.parse().unwrap_or(0.0),
                    high: c.mid.h.parse().unwrap_or(0.0),
                    low: c.mid.l.parse().unwrap_or(0.0),
                    close: c.mid.c.parse().unwrap_or(0.0),
                    volume: Some(c.volume as f64),
                })
                .collect();
            
            tracing::info!("Received {} candles in chunk {}", batch_candles.len(), iteration + 1);
            all_candles.extend(batch_candles);
            
            // Move to next chunk
            chunk_start = chunk_end;
        }

        tracing::info!("Total fetched: {} candles from {} to {}", all_candles.len(), from, to);
        Ok(all_candles)
    }

    /// Get current price for an instrument
    pub async fn get_price(&self, instrument: &str) -> Result<PriceInfo, AppError> {
        let url = format!(
            "{}/v3/accounts/{}/pricing?instruments={}",
            self.base_url, self.account_id, instrument
        );

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: PricingResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        data.prices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::NotFound("No price data".to_string()))
    }

    /// Get open positions
    pub async fn get_positions(&self) -> Result<Vec<OandaPosition>, AppError> {
        let url = format!("{}/v3/accounts/{}/openPositions", self.base_url, self.account_id);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: PositionsResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        Ok(data.positions)
    }
    
    /// Get open positions (alias for bot runner)
    pub async fn get_open_positions(&self) -> Result<Vec<OpenPosition>, AppError> {
        let positions = self.get_positions().await?;
        
        let open: Vec<OpenPosition> = positions.into_iter()
            .filter_map(|p| {
                let long_units: f64 = p.long.units.parse().unwrap_or(0.0);
                let short_units: f64 = p.short.units.parse().unwrap_or(0.0);
                
                if long_units != 0.0 {
                    Some(OpenPosition {
                        instrument: p.instrument.clone(),
                        units: long_units,
                        direction: "LONG".to_string(),
                        average_price: p.long.average_price.clone().unwrap_or_default(),
                        unrealized_pl: p.long.unrealized_pl.parse().unwrap_or(0.0),
                    })
                } else if short_units != 0.0 {
                    Some(OpenPosition {
                        instrument: p.instrument.clone(),
                        units: short_units.abs(),
                        direction: "SHORT".to_string(),
                        average_price: p.short.average_price.clone().unwrap_or_default(),
                        unrealized_pl: p.short.unrealized_pl.parse().unwrap_or(0.0),
                    })
                } else {
                    None
                }
            })
            .collect();
        
        Ok(open)
    }
    
    /// Place a market order
    pub async fn place_order(
        &self,
        instrument: &str,
        direction: &str,
        lot_size: f64,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<OrderResponse, AppError> {
        let url = format!("{}/v3/accounts/{}/orders", self.base_url, self.account_id);
        
        // Convert lot size to OANDA units (instrument-aware)
        let multiplier = units_per_lot(instrument);
        let units = if direction == "SELL" || direction == "SHORT" {
            -(lot_size * multiplier) as i64
        } else {
            (lot_size * multiplier) as i64
        };
        
        // Build the order request
        let mut order = serde_json::json!({
            "order": {
                "type": "MARKET",
                "instrument": instrument,
                "units": units.to_string(),
                "timeInForce": "FOK",
                "positionFill": "DEFAULT"
            }
        });
        
        // Add stop loss if provided
        if let Some(sl) = stop_loss {
            order["order"]["stopLossOnFill"] = serde_json::json!({
                "price": format!("{:.2}", sl)
            });
        }
        
        // Add take profit if provided
        if let Some(tp) = take_profit {
            order["order"]["takeProfitOnFill"] = serde_json::json!({
                "price": format!("{:.2}", tp)
            });
        }
        
        tracing::info!("Placing order: {:?}", order);
        
        let response = self.client
            .post(&url)
            .json(&order)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Order failed: {}", error_text);
            return Err(AppError::InternalError(format!("Order failed: {}", error_text)));
        }
        
        let data: CreateOrderResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;
        
        Ok(OrderResponse {
            id: data.order_fill_transaction.as_ref()
                .map(|t| t.id.clone())
                .or_else(|| data.order_create_transaction.as_ref().map(|t| t.id.clone()))
                .unwrap_or_default(),
            instrument: instrument.to_string(),
            units,
            price: data.order_fill_transaction.as_ref()
                .and_then(|t| t.price.as_ref())
                .and_then(|p| p.parse().ok())
                .unwrap_or(0.0),
        })
    }
    
    /// Get recent trades (open and closed)
    pub async fn get_trades(&self, count: u32, state: Option<&str>) -> Result<Vec<Trade>, AppError> {
        let state_param = state.unwrap_or("ALL"); // ALL, OPEN, CLOSED
        let url = format!(
            "{}/v3/accounts/{}/trades?count={}&state={}",
            self.base_url, self.account_id, count, state_param
        );

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: TradesResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        Ok(data.trades)
    }
    
    /// Get open trades only
    pub async fn get_open_trades(&self) -> Result<Vec<Trade>, AppError> {
        self.get_trades(50, Some("OPEN")).await
    }
    
    /// Get closed trades (recent)
    pub async fn get_closed_trades(&self, count: u32) -> Result<Vec<Trade>, AppError> {
        self.get_trades(count, Some("CLOSED")).await
    }
    
    /// Get a specific trade by ID
    pub async fn get_trade(&self, trade_id: &str) -> Result<Trade, AppError> {
        let url = format!(
            "{}/v3/accounts/{}/trades/{}",
            self.base_url, self.account_id, trade_id
        );

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::InternalError(format!("OANDA error: {}", error_text)));
        }

        let data: TradeResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse response: {}", e)))?;

        Ok(data.trade)
    }

    /// Modify stop loss and/or take profit on an open trade
    pub async fn modify_trade(
        &self,
        trade_id: &str,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<(), AppError> {
        let url = format!(
            "{}/v3/accounts/{}/trades/{}/orders",
            self.base_url, self.account_id, trade_id
        );

        let mut body = serde_json::json!({});

        if let Some(sl) = stop_loss {
            body["stopLoss"] = serde_json::json!({
                "price": format!("{:.2}", sl),
                "timeInForce": "GTC"
            });
        }

        if let Some(tp) = take_profit {
            body["takeProfit"] = serde_json::json!({
                "price": format!("{:.2}", tp),
                "timeInForce": "GTC"
            });
        }

        tracing::info!("Modifying trade {}: {:?}", trade_id, body);

        let response = self.client
            .put(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA modify request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Trade modify failed: {}", error_text);
            return Err(AppError::InternalError(format!("Trade modify failed: {}", error_text)));
        }

        tracing::info!("✅ Trade {} SL/TP modified successfully", trade_id);
        Ok(())
    }

    /// Close a specific trade by ID
    pub async fn close_trade(&self, trade_id: &str) -> Result<(), AppError> {
        let url = format!(
            "{}/v3/accounts/{}/trades/{}/close",
            self.base_url, self.account_id, trade_id
        );

        let response = self.client
            .put(&url)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| AppError::InternalError(format!("OANDA close request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Trade close failed: {}", error_text);
            return Err(AppError::InternalError(format!("Trade close failed: {}", error_text)));
        }

        tracing::info!("✅ Trade {} closed successfully", trade_id);
        Ok(())
    }
}

/// Convert lot size to OANDA units for a given instrument.
///
/// In standard trading terminology:
/// - 1 "lot" of Gold = 100 troy ounces
/// - 1 "lot" of Forex = 100,000 base currency units
/// - 1 "lot" of BTC = 1 BTC
/// - 1 "lot" of Oil = 100 barrels
/// - 1 "lot" of NatGas = 10,000 MMBtu
///
/// So `lot_size = 0.01` for gold = 1 oz, for forex = 1,000 units (micro lot), etc.
pub fn units_per_lot(instrument: &str) -> f64 {
    match instrument {
        "XAU_USD" => 100.0,        // 1 lot = 100 troy ounces
        "BTC_USD" => 1.0,          // 1 lot = 1 BTC
        "EUR_USD" => 100_000.0,    // 1 lot = 100,000 EUR
        "USD_JPY" => 100_000.0,    // 1 lot = 100,000 USD
        "GBP_USD" => 100_000.0,    // 1 lot = 100,000 GBP
        "AUD_USD" => 100_000.0,    // 1 lot = 100,000 AUD
        "WTICO_USD" => 100.0,      // 1 lot = 100 barrels
        "NATGAS_USD" => 10_000.0,  // 1 lot = 10,000 MMBtu
        _ => {
            // Default: assume forex-like (100k units)
            tracing::warn!("Unknown instrument '{}', defaulting to 100,000 units/lot", instrument);
            100_000.0
        }
    }
}

fn parse_oanda_time(time_str: &str) -> i64 {
    // OANDA returns times like "2024-01-15T10:30:00.000000000Z"
    chrono::DateTime::parse_from_rfc3339(time_str)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

// OANDA API Response Types

#[derive(Debug, Deserialize)]
pub struct AccountSummaryResponse {
    pub account: AccountSummary,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AccountSummary {
    pub id: String,
    pub balance: String,
    pub currency: String,
    #[serde(rename = "pl")]
    pub profit_loss: String,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: String,
    #[serde(rename = "NAV")]
    pub nav: String,
    #[serde(rename = "marginUsed")]
    pub margin_used: String,
    #[serde(rename = "marginAvailable")]
    pub margin_available: String,
    #[serde(rename = "openTradeCount")]
    pub open_trade_count: i32,
    #[serde(rename = "openPositionCount")]
    pub open_position_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct CandlesResponse {
    pub instrument: String,
    pub granularity: String,
    pub candles: Vec<OandaCandle>,
}

#[derive(Debug, Deserialize)]
pub struct OandaCandle {
    pub complete: bool,
    pub time: String,
    pub volume: i64,
    pub mid: OandaCandleMid,
}

#[derive(Debug, Deserialize)]
pub struct OandaCandleMid {
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
}

#[derive(Debug, Deserialize)]
pub struct PricingResponse {
    pub prices: Vec<PriceInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PriceInfo {
    pub instrument: String,
    pub time: String,
    pub bids: Vec<PriceBucket>,
    pub asks: Vec<PriceBucket>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PriceBucket {
    pub price: String,
    pub liquidity: i64,
}

#[derive(Debug, Deserialize)]
pub struct PositionsResponse {
    pub positions: Vec<OandaPosition>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OandaPosition {
    pub instrument: String,
    pub long: PositionSide,
    pub short: PositionSide,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PositionSide {
    pub units: String,
    #[serde(rename = "averagePrice")]
    pub average_price: Option<String>,
    pub pl: String,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: String,
}

// Additional types for bot runner

#[derive(Debug, Clone)]
pub struct OpenPosition {
    pub instrument: String,
    pub units: f64,
    pub direction: String,
    pub average_price: String,
    pub unrealized_pl: f64,
}

#[derive(Debug, Clone)]
pub struct OrderResponse {
    pub id: String,
    pub instrument: String,
    pub units: i64,
    pub price: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderResponse {
    #[serde(rename = "orderFillTransaction")]
    pub order_fill_transaction: Option<OrderFillTransaction>,
    #[serde(rename = "orderCreateTransaction")]
    pub order_create_transaction: Option<OrderCreateTransaction>,
}

#[derive(Debug, Deserialize)]
pub struct OrderFillTransaction {
    pub id: String,
    pub price: Option<String>,
    pub units: Option<String>,
    pub pl: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OrderCreateTransaction {
    pub id: String,
    pub instrument: Option<String>,
    pub units: Option<String>,
}

// Trade history types
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Trade {
    pub id: String,
    pub instrument: String,
    #[serde(rename = "initialUnits")]
    pub initial_units: String,
    #[serde(rename = "currentUnits")]
    pub current_units: String,
    pub price: String,
    #[serde(rename = "openTime")]
    pub open_time: String,
    pub state: String,  // OPEN, CLOSED, CLOSE_WHEN_TRADEABLE
    #[serde(rename = "realizedPL")]
    pub realized_pl: Option<String>,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: Option<String>,
    #[serde(rename = "closingTransactionIDs")]
    pub closing_transaction_ids: Option<Vec<String>>,
    #[serde(rename = "closeTime")]
    pub close_time: Option<String>,
    #[serde(rename = "averageClosePrice")]
    pub average_close_price: Option<String>,
    #[serde(rename = "stopLossOrder")]
    pub stop_loss_order: Option<StopLossOrder>,
    #[serde(rename = "takeProfitOrder")]
    pub take_profit_order: Option<TakeProfitOrder>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StopLossOrder {
    pub id: String,
    pub price: String,
    pub state: Option<String>,  // PENDING, FILLED, CANCELLED
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TakeProfitOrder {
    pub id: String,
    pub price: String,
    pub state: Option<String>,  // PENDING, FILLED, CANCELLED
}

#[derive(Debug, Deserialize)]
pub struct TradesResponse {
    pub trades: Vec<Trade>,
}

#[derive(Debug, Deserialize)]
pub struct TradeResponse {
    pub trade: Trade,
}
