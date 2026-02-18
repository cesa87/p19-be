use actix_web::{web, HttpResponse, Result};

use crate::broker::OandaClient;
use crate::config::Config;
use crate::error::AppError;
use crate::models::Candle;

#[derive(serde::Deserialize)]
pub struct HistoricalDataQuery {
    timeframe: Option<String>,
    count: Option<u32>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/market-data")
            .route("/historical/{symbol}", web::get().to(get_historical))
            .route("/price/{symbol}", web::get().to(get_price))
            .route("/symbols", web::get().to(get_symbols))
    );
}

async fn get_historical(
    path: web::Path<String>,
    query: web::Query<HistoricalDataQuery>,
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    let symbol = path.into_inner();
    // Convert symbol format: XAUUSD -> XAU_USD for OANDA
    let oanda_symbol = convert_to_oanda_symbol(&symbol);
    let timeframe = query.timeframe.clone().unwrap_or("H4".to_string());
    let count = query.count.unwrap_or(100);
    
    // Try to use OANDA if configured
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let candles = client.get_candles(&oanda_symbol, &timeframe, count).await?;
        return Ok(HttpResponse::Ok().json(candles));
    }
    
    // Fallback to mock data if OANDA not configured
    let candles = generate_mock_candles(&symbol, &timeframe, count);
    Ok(HttpResponse::Ok().json(candles))
}

async fn get_price(
    path: web::Path<String>,
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    let symbol = path.into_inner();
    let oanda_symbol = convert_to_oanda_symbol(&symbol);
    
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        let price = client.get_price(&oanda_symbol).await?;
        
        let bid: f64 = price.bids.first().map(|b| b.price.parse().unwrap_or(0.0)).unwrap_or(0.0);
        let ask: f64 = price.asks.first().map(|a| a.price.parse().unwrap_or(0.0)).unwrap_or(0.0);
        
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "symbol": symbol,
            "bid": bid,
            "ask": ask,
            "mid": (bid + ask) / 2.0,
            "spread": ask - bid,
            "time": price.time
        })));
    }
    
    // Mock price if OANDA not configured
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "symbol": symbol,
        "bid": 2042.50,
        "ask": 2042.80,
        "mid": 2042.65,
        "spread": 0.30
    })))
}

fn convert_to_oanda_symbol(symbol: &str) -> String {
    // If already has underscore, return as-is
    if symbol.contains('_') {
        return symbol.to_string();
    }
    
    // Handle special OANDA symbols that end with USD but aren't 6 chars
    // WTICOUSD -> WTICO_USD, NAS100USD -> NAS100_USD, SPX500USD -> SPX500_USD, NATGASUSD -> NATGAS_USD
    if symbol.ends_with("USD") && symbol.len() > 6 {
        let base = &symbol[..symbol.len() - 3];
        return format!("{}_USD", base);
    }
    
    // Handle standard 6-char forex/commodity symbols
    // XAUUSD -> XAU_USD, EURUSD -> EUR_USD, BTCUSD -> BTC_USD
    if symbol.len() == 6 {
        return format!("{}_{}", &symbol[..3], &symbol[3..]);
    }
    
    // Return as-is if no conversion rule matches
    symbol.to_string()
}

async fn get_symbols() -> Result<HttpResponse, AppError> {
    let symbols = vec![
        // Commodities
        serde_json::json!({
            "symbol": "XAU_USD",
            "name": "Gold / US Dollar",
            "type": "commodity",
            "category": "precious_metal"
        }),
        serde_json::json!({
            "symbol": "WTICO_USD",
            "name": "WTI Crude Oil",
            "type": "commodity",
            "category": "energy"
        }),
        serde_json::json!({
            "symbol": "NATGAS_USD",
            "name": "Natural Gas",
            "type": "commodity",
            "category": "energy"
        }),
        // Crypto
        serde_json::json!({
            "symbol": "BTC_USD",
            "name": "Bitcoin / US Dollar",
            "type": "crypto",
            "category": "crypto"
        }),
        // Forex Majors
        serde_json::json!({
            "symbol": "EUR_USD",
            "name": "Euro / US Dollar",
            "type": "forex",
            "category": "major"
        }),
        serde_json::json!({
            "symbol": "USD_JPY",
            "name": "US Dollar / Japanese Yen",
            "type": "forex",
            "category": "major"
        }),
    ];
    
    Ok(HttpResponse::Ok().json(symbols))
}

fn generate_mock_candles(_symbol: &str, timeframe: &str, count: u32) -> Vec<Candle> {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    let interval_seconds: i64 = match timeframe {
        "H1" | "1H" => 3600,
        "H4" | "4H" => 4 * 3600,
        "D" | "1D" => 24 * 3600,
        _ => 4 * 3600,
    };
    
    let num_candles = count as i64;
    let mut candles = Vec::new();
    let mut base_price = 2040.0;
    
    for i in (0..num_candles).rev() {
        let time = now - (i * interval_seconds);
        let volatility = 5.0 + (rand_float() * 10.0);
        let open = base_price + (rand_float() - 0.5) * volatility;
        let close = open + (rand_float() - 0.5) * volatility;
        let high = open.max(close) + rand_float() * volatility * 0.5;
        let low = open.min(close) - rand_float() * volatility * 0.5;
        
        candles.push(Candle {
            time,
            open: (open * 100.0).round() / 100.0,
            high: (high * 100.0).round() / 100.0,
            low: (low * 100.0).round() / 100.0,
            close: (close * 100.0).round() / 100.0,
            volume: Some(rand_float() * 10000.0),
        });
        
        base_price = close;
    }
    
    candles
}

// Simple pseudo-random using system time
fn rand_float() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos as f64 % 1000.0) / 1000.0
}
