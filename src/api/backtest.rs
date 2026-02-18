use actix_web::{web, HttpResponse, Result};
use uuid::Uuid;

use crate::broker::OandaClient;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{BacktestRequest, BacktestResult, BacktestMetrics, Candle, Strategy, StrategyResponse};
use crate::engine::backtest::run_backtest;
use crate::engine::strategy::templates;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/backtest")
            .route("", web::post().to(execute_backtest))
            .route("/strategies", web::get().to(list_strategies))
            .route("/results/{id}", web::get().to(get_result))
            .route("/history", web::get().to(list_results))
    );
}

/// List available strategies (user-created + templates)
async fn list_strategies(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    // Fetch all strategies from database
    // TODO: Filter by user_id from JWT when auth is implemented
    let db_strategies = sqlx::query_as::<_, Strategy>(
        "SELECT * FROM strategies ORDER BY created_at DESC"
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap_or_default();
    
    let strategies: Vec<serde_json::Value> = db_strategies
        .into_iter()
        .map(|s| {
            // Extract SL/TP - first try params (legacy), then risk object (new format)
            let sl_pips = s.params.get("sl_pips")
                .and_then(|v| v.as_f64())
                .or_else(|| s.risk.get("stop_loss_pct").and_then(|v| v.as_f64()))
                .unwrap_or(1.0);  // Default 1%
            let tp_pips = s.params.get("tp1_pips")
                .and_then(|v| v.as_f64())
                .or_else(|| s.params.get("tp_pips").and_then(|v| v.as_f64()))
                .or_else(|| s.risk.get("take_profit_pct").and_then(|v| v.as_f64()))
                .unwrap_or(2.0);  // Default 2%
            
            let response: StrategyResponse = s.into();
            serde_json::json!({
                "id": response.id,
                "name": response.name,
                "description": format!("{} strategy", response.strategy_type),
                "indicators": get_indicators_for_type(&response.strategy_type),
                "stop_loss_pips": sl_pips,
                "take_profit_pips": tp_pips
            })
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(strategies))
}

fn get_indicators_for_type(strategy_type: &str) -> Vec<&'static str> {
    match strategy_type.to_lowercase().as_str() {
        "sma_crossover" | "ma_crossover" => vec!["SMA(9)", "SMA(21)"],
        "macd_crossover" => vec!["MACD(12,26,9)"],
        "macd_divergence" => vec!["MACD(12,26,9)", "Divergence"],
        "donchian_breakout" => vec!["Donchian(20)"],
        "adx_trend" => vec!["ADX(14)", "+DI", "-DI"],
        "rsi" | "rsi_reversal" | "rsi_mean_reversion" => vec!["RSI(14)"],
        "bollinger" | "bollinger_bands" | "bollinger_mean_reversion" => vec!["BB(20,2)"],
        "stochastic_crossover" => vec!["Stoch(14,3)"],
        "ema_ribbon" => vec!["EMA(8,13,21,34,55)"],
        "triple_screen" => vec!["EMA(13)", "Stoch(5,3)"],
        "london_breakout" => vec!["Range Breakout"],
        "ema_crossover" => vec!["EMA(9)", "EMA(21)"],
        _ => vec!["Custom"]
    }
}

async fn execute_backtest(
    body: web::Json<BacktestRequest>,
    config: web::Data<Config>,
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    // Fetch historical data from OANDA
    let candles = fetch_backtest_data(
        &config, 
        &body.symbol, 
        &body.timeframe, 
        body.candle_count,
        body.start_date.as_deref(),
        body.end_date.as_deref(),
    ).await?;
    
    // Fetch strategy from database
    let uuid = Uuid::parse_str(&body.strategy_id)
        .map_err(|_| AppError::BadRequest("Invalid strategy ID".to_string()))?;
    
    let strategy = sqlx::query_as::<_, Strategy>(
        "SELECT * FROM strategies WHERE id = $1"
    )
    .bind(uuid)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Strategy not found".to_string()))?;
    
    // Extract SL/TP - first try params (legacy pips), then risk object (percentage)
    let sl_pips = strategy.params.get("sl_pips")
        .and_then(|v| v.as_f64())
        .or_else(|| strategy.risk.get("stop_loss_pct").and_then(|v| v.as_f64()))
        .unwrap_or(1.0);  // Default 1%
    let tp_pips = strategy.params.get("tp1_pips")
        .and_then(|v| v.as_f64())
        .or_else(|| strategy.params.get("tp_pips").and_then(|v| v.as_f64()))
        .or_else(|| strategy.risk.get("take_profit_pct").and_then(|v| v.as_f64()))
        .unwrap_or(2.0);  // Default 2%
    
    tracing::info!("Backtest with SL: {}%, TP: {}%", sl_pips, tp_pips);
    
    // Run the backtest engine with actual strategy params
    let result = crate::engine::backtest::run_backtest_with_params(
        &body.symbol,
        &strategy.strategy_type,
        sl_pips,
        tp_pips,
        candles,
        body.initial_capital,
    ).await?;
    
    Ok(HttpResponse::Ok().json(result))
}

/// Fetch historical candles for backtesting
async fn fetch_backtest_data(
    config: &Config,
    symbol: &str,
    timeframe: &str,
    count: Option<u32>,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<Vec<Candle>, AppError> {
    // Convert symbol format: XAUUSD -> XAU_USD for OANDA
    let oanda_symbol = if symbol.len() == 6 {
        format!("{}_{}", &symbol[..3], &symbol[3..])
    } else {
        symbol.to_string()
    };
    
    // Try to use OANDA if configured
    if let (Some(token), Some(account_id)) = (&config.oanda_api_token, &config.oanda_account_id) {
        let client = OandaClient::new(token, account_id, config.oanda_practice)?;
        
        // Use date range if provided, otherwise use count
        if let (Some(start), Some(end)) = (start_date, end_date) {
            tracing::info!("Fetching candles from {} to {}", start, end);
            return client.get_candles_by_date(&oanda_symbol, timeframe, start, end).await;
        } else {
            let count = count.unwrap_or(500);
            return client.get_candles(&oanda_symbol, timeframe, count).await;
        }
    }
    
    // Return empty if OANDA not configured (will use generated data)
    Ok(vec![])
}

async fn get_result(
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    // TODO: Fetch from database
    // For now, return a mock result
    let result = BacktestResult {
        id,
        strategy_id: "mock".to_string(),
        metrics: BacktestMetrics {
            total_return: 15.5,
            sharpe_ratio: 1.8,
            max_drawdown: 8.2,
            win_rate: 62.5,
            total_trades: 48,
            profit_factor: 1.65,
            average_win: 125.0,
            average_loss: 75.0,
        },
        equity_curve: vec![],
        trades: vec![],
        executed_at: chrono::Utc::now().to_rfc3339(),
    };
    
    Ok(HttpResponse::Ok().json(result))
}

async fn list_results() -> Result<HttpResponse, AppError> {
    // TODO: Fetch from database
    let results: Vec<BacktestResult> = vec![];
    
    Ok(HttpResponse::Ok().json(results))
}
