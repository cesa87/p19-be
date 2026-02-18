use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestRequest {
    pub strategy_id: String,
    #[serde(default = "default_symbol")]
    pub symbol: String,
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    #[serde(default)]
    pub candle_count: Option<u32>,
    /// Start date for custom date range (ISO 8601)
    #[serde(default)]
    pub start_date: Option<String>,
    /// End date for custom date range (ISO 8601)
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default = "default_capital")]
    pub initial_capital: f64,
}

fn default_symbol() -> String { "XAUUSD".to_string() }
fn default_timeframe() -> String { "H4".to_string() }
fn default_capital() -> f64 { 10000.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub total_trades: usize,
    pub profit_factor: f64,
    pub average_win: f64,
    pub average_loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub time: i64,
    pub equity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub id: String,
    pub strategy_id: String,
    pub metrics: BacktestMetrics,
    pub equity_curve: Vec<EquityPoint>,
    pub trades: Vec<super::TradeResponse>,
    pub executed_at: String,
}
