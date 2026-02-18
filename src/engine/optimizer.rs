use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::broker::OandaClient;
use crate::config::Config;
use crate::error::AppError;
use crate::models::{BacktestMetrics, Candle};
use crate::engine::backtest::run_backtest_with_params;

// ============================================================================
// Configuration
// ============================================================================

/// All strategies that have backtest simulations in backtest.rs
pub const BACKTESTABLE_STRATEGIES: &[&str] = &[
    "sma_crossover",
    "macd_crossover",
    "macd_divergence",
    "donchian_breakout",
    "adx_trend",
    "rsi_mean_reversion",
    "bollinger_mean_reversion",
    "stochastic_crossover",
    "ema_ribbon",
    "triple_screen",
    "london_breakout",
    "quant_gold_momentum",
    "rsi_reversion_2",
    "london_ny_trend_continuation",
    "london_ny_trend_2",
    "forecast_confidence",
    "volatility_expansion",
    "trendline_bounce",
    "liquidity_sweep",
];

/// Quick-scan subset (most common/promising strategies)
pub const QUICK_STRATEGIES: &[&str] = &[
    "sma_crossover",
    "macd_crossover",
    "rsi_mean_reversion",
    "bollinger_mean_reversion",
    "donchian_breakout",
    "rsi_reversion_2",
    "liquidity_sweep",
    "trendline_bounce",
];

pub const DEFAULT_INSTRUMENTS: &[&str] = &[
    "XAU_USD",
    "BTC_USD",
    "EUR_USD",
    "GBP_USD",
    "USD_JPY",
    "WTICO_USD",
    "NAS100_USD",
    "SPX500_USD",
];
pub const QUICK_INSTRUMENTS: &[&str] = &["XAU_USD", "EUR_USD", "BTC_USD"];

pub const DEFAULT_TIMEFRAMES: &[&str] = &["M15", "H1", "H4", "D"];
pub const QUICK_TIMEFRAMES: &[&str] = &["H1", "H4"];

// NOTE: These are base defaults; the optimizer auto-scales per instrument+timeframe.
// For gold (PIP_VALUE=0.1), 100 pips = $10.00 price movement.
// Gold H1 ATR is typically 80-150 pips, H4 ATR 150-300 pips.
pub const DEFAULT_SL_PIPS: &[f64] = &[50.0, 80.0, 100.0, 150.0, 200.0, 300.0, 500.0];
pub const DEFAULT_TP_PIPS: &[f64] = &[100.0, 150.0, 200.0, 300.0, 400.0, 500.0, 800.0];

pub const QUICK_SL_PIPS: &[f64] = &[100.0, 200.0, 300.0];
pub const QUICK_TP_PIPS: &[f64] = &[200.0, 400.0, 600.0];

const MIN_TRADES_THRESHOLD: usize = 10;
const DEFAULT_INITIAL_CAPITAL: f64 = 10000.0;
const DEFAULT_CANDLE_COUNT: u32 = 1000;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    #[serde(default = "default_strategies")]
    pub strategies: Vec<String>,
    #[serde(default = "default_instruments")]
    pub instruments: Vec<String>,
    #[serde(default = "default_timeframes")]
    pub timeframes: Vec<String>,
    #[serde(default = "default_sl_pips")]
    pub sl_pips: Vec<f64>,
    #[serde(default = "default_tp_pips")]
    pub tp_pips: Vec<f64>,
    #[serde(default = "default_candle_count")]
    pub candle_count: u32,
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
}

fn default_strategies() -> Vec<String> { BACKTESTABLE_STRATEGIES.iter().map(|s| s.to_string()).collect() }
fn default_instruments() -> Vec<String> { DEFAULT_INSTRUMENTS.iter().map(|s| s.to_string()).collect() }
fn default_timeframes() -> Vec<String> { DEFAULT_TIMEFRAMES.iter().map(|s| s.to_string()).collect() }
fn default_sl_pips() -> Vec<f64> { DEFAULT_SL_PIPS.to_vec() }
fn default_tp_pips() -> Vec<f64> { DEFAULT_TP_PIPS.to_vec() }
fn default_candle_count() -> u32 { DEFAULT_CANDLE_COUNT }
fn default_initial_capital() -> f64 { DEFAULT_INITIAL_CAPITAL }

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            strategies: default_strategies(),
            instruments: default_instruments(),
            timeframes: default_timeframes(),
            sl_pips: default_sl_pips(),
            tp_pips: default_tp_pips(),
            candle_count: DEFAULT_CANDLE_COUNT,
            initial_capital: DEFAULT_INITIAL_CAPITAL,
        }
    }
}

impl OptimizationConfig {
    pub fn quick() -> Self {
        Self {
            strategies: QUICK_STRATEGIES.iter().map(|s| s.to_string()).collect(),
            instruments: QUICK_INSTRUMENTS.iter().map(|s| s.to_string()).collect(),
            timeframes: QUICK_TIMEFRAMES.iter().map(|s| s.to_string()).collect(),
            sl_pips: QUICK_SL_PIPS.to_vec(),
            tp_pips: QUICK_TP_PIPS.to_vec(),
            candle_count: 500,
            initial_capital: DEFAULT_INITIAL_CAPITAL,
        }
    }

    /// Total combinations that will be tested
    pub fn total_combinations(&self) -> usize {
        let valid_sl_tp = self.sl_pips.iter()
            .flat_map(|sl| self.tp_pips.iter().filter(move |tp| **tp >= *sl))
            .count();
        self.strategies.len() * self.instruments.len() * self.timeframes.len() * valid_sl_tp
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedResult {
    pub rank: usize,
    pub strategy: String,
    pub instrument: String,
    pub timeframe: String,
    pub sl_pips: f64,
    pub tp_pips: f64,
    pub metrics: BacktestMetrics,
    pub composite_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyBest {
    pub strategy: String,
    pub best_result: OptimizedResult,
    pub profitable_combos: usize,
    pub total_combos: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentBest {
    pub instrument: String,
    pub best_result: OptimizedResult,
    pub profitable_combos: usize,
    pub total_combos: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerReport {
    pub total_combinations_tested: usize,
    pub profitable_combinations: usize,
    pub top_results: Vec<OptimizedResult>,
    pub best_per_strategy: Vec<StrategyBest>,
    pub best_per_instrument: Vec<InstrumentBest>,
    pub duration_seconds: f64,
    pub config: OptimizationConfig,
}

// ============================================================================
// Composite Score
// ============================================================================

/// Compute a composite score from backtest metrics.
/// Weights: 40% Sharpe, 30% profit factor, 20% win rate, 10% return.
/// All inputs are normalized to roughly 0-100 scale.
fn composite_score(m: &BacktestMetrics) -> f64 {
    // Sharpe: typically -2 to +3, normalize to 0-100
    let sharpe_norm = ((m.sharpe_ratio + 2.0) / 5.0 * 100.0).clamp(0.0, 100.0);
    // Profit factor: 0 to 3+, normalize to 0-100
    let pf_norm = (m.profit_factor / 3.0 * 100.0).clamp(0.0, 100.0);
    // Win rate: already 0-100
    let wr_norm = m.win_rate.clamp(0.0, 100.0);
    // Total return: -50 to +50 typical, normalize to 0-100
    let ret_norm = ((m.total_return + 50.0) / 100.0 * 100.0).clamp(0.0, 100.0);

    let score = 0.4 * sharpe_norm + 0.3 * pf_norm + 0.2 * wr_norm + 0.1 * ret_norm;
    (score * 100.0).round() / 100.0
}

// ============================================================================
// Timeframe-aware parameter scaling
// ============================================================================

/// Returns a multiplier for SL/TP values based on timeframe.
/// Shorter timeframes need tighter stops, longer ones need wider.
/// Base values are calibrated for H1.
fn timeframe_multiplier(tf: &str) -> f64 {
    match tf {
        "M1"  => 0.15,
        "M5"  => 0.25,
        "M15" => 0.4,
        "M30" => 0.6,
        "H1"  => 1.0,
        "H4"  => 2.0,
        "D"   => 5.0,
        "W"   => 12.0,
        _     => 1.0,
    }
}

/// Scale SL/TP arrays by a timeframe multiplier, rounding to nearest integer.
fn scale_pips(base: &[f64], mult: f64) -> Vec<f64> {
    base.iter()
        .map(|v| (v * mult).round())
        .collect()
}

// ============================================================================
// Optimizer Engine
// ============================================================================

/// Pre-fetch candle data for all instrument+timeframe combinations.
/// Returns a map keyed by "INSTRUMENT:TIMEFRAME".
async fn prefetch_candles(
    config: &Config,
    instruments: &[String],
    timeframes: &[String],
    count: u32,
) -> Result<HashMap<String, Vec<Candle>>, AppError> {
    let mut candle_cache: HashMap<String, Vec<Candle>> = HashMap::new();

    let (token, account_id) = match (&config.oanda_api_token, &config.oanda_account_id) {
        (Some(t), Some(a)) => (t.clone(), a.clone()),
        _ => return Err(AppError::InternalError("OANDA not configured".to_string())),
    };

    let client = OandaClient::new(&token, &account_id, config.oanda_practice)?;

    for instrument in instruments {
        for timeframe in timeframes {
            let key = format!("{}:{}", instrument, timeframe);
            tracing::info!("Fetching {} candles for {} @ {}", count, instrument, timeframe);

            match client.get_candles(instrument, timeframe, count).await {
                Ok(candles) => {
                    tracing::info!("Got {} candles for {}", candles.len(), key);
                    candle_cache.insert(key, candles);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch candles for {}: {}", key, e);
                    // Continue with other combos rather than failing entirely
                }
            }
        }
    }

    if candle_cache.is_empty() {
        return Err(AppError::InternalError(
            "Could not fetch candle data for any instrument/timeframe combination".to_string(),
        ));
    }

    Ok(candle_cache)
}

/// Run the full optimization sweep.
pub async fn run_optimization(
    config: &Config,
    opt_config: OptimizationConfig,
) -> Result<OptimizerReport, AppError> {
    let start = std::time::Instant::now();

    tracing::info!(
        "Starting optimization: {} strategies × {} instruments × {} timeframes | {} estimated combos",
        opt_config.strategies.len(),
        opt_config.instruments.len(),
        opt_config.timeframes.len(),
        opt_config.total_combinations(),
    );

    // 1. Pre-fetch all candle data
    let candle_cache = prefetch_candles(
        config,
        &opt_config.instruments,
        &opt_config.timeframes,
        opt_config.candle_count,
    )
    .await?;

    // 2. Run parameter sweep
    let mut all_results: Vec<OptimizedResult> = Vec::new();
    let mut tested = 0usize;

    for strategy in &opt_config.strategies {
        for instrument in &opt_config.instruments {
            // Convert OANDA instrument format to symbol for backtest
            let symbol = instrument.replace('_', "");

            for timeframe in &opt_config.timeframes {
                let cache_key = format!("{}:{}", instrument, timeframe);
                let candles = match candle_cache.get(&cache_key) {
                    Some(c) if c.len() >= 50 => c.clone(),
                    Some(c) => {
                        tracing::warn!("Skipping {} — only {} candles (need 50+)", cache_key, c.len());
                        continue;
                    }
                    None => continue,
                };

                // Scale SL/TP by timeframe volatility
                let tf_mult = timeframe_multiplier(timeframe);
                let scaled_sl = scale_pips(&opt_config.sl_pips, tf_mult);
                let scaled_tp = scale_pips(&opt_config.tp_pips, tf_mult);

                for &sl in &scaled_sl {
                    for &tp in &scaled_tp {
                        // Skip negative expectancy combos (TP < SL)
                        if tp < sl {
                            continue;
                        }

                        let result = run_backtest_with_params(
                            &symbol,
                            strategy,
                            sl,
                            tp,
                            candles.clone(),
                            opt_config.initial_capital,
                        )
                        .await?;

                        tested += 1;

                        // Filter out low-trade-count results
                        if result.metrics.total_trades < MIN_TRADES_THRESHOLD {
                            continue;
                        }

                        let score = composite_score(&result.metrics);

                        all_results.push(OptimizedResult {
                            rank: 0, // set later
                            strategy: strategy.clone(),
                            instrument: instrument.clone(),
                            timeframe: timeframe.clone(),
                            sl_pips: sl,
                            tp_pips: tp,
                            metrics: result.metrics,
                            composite_score: score,
                        });
                    }
                }
            }
        }
    }

    // 3. Sort by composite score descending
    all_results.sort_by(|a, b| b.composite_score.partial_cmp(&a.composite_score).unwrap_or(std::cmp::Ordering::Equal));

    // Assign ranks
    for (i, r) in all_results.iter_mut().enumerate() {
        r.rank = i + 1;
    }

    // 4. Count profitable
    let profitable = all_results.iter().filter(|r| r.metrics.total_return > 0.0).count();

    // 5. Best per strategy
    let mut strategy_map: HashMap<String, Vec<&OptimizedResult>> = HashMap::new();
    for r in &all_results {
        strategy_map.entry(r.strategy.clone()).or_default().push(r);
    }
    let mut best_per_strategy: Vec<StrategyBest> = strategy_map
        .into_iter()
        .filter_map(|(strat, results)| {
            let profitable_count = results.iter().filter(|r| r.metrics.total_return > 0.0).count();
            let total = results.len();
            results.first().map(|best| StrategyBest {
                strategy: strat,
                best_result: (*best).clone(),
                profitable_combos: profitable_count,
                total_combos: total,
            })
        })
        .collect();
    best_per_strategy.sort_by(|a, b| {
        b.best_result.composite_score
            .partial_cmp(&a.best_result.composite_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 6. Best per instrument
    let mut instrument_map: HashMap<String, Vec<&OptimizedResult>> = HashMap::new();
    for r in &all_results {
        instrument_map.entry(r.instrument.clone()).or_default().push(r);
    }
    let mut best_per_instrument: Vec<InstrumentBest> = instrument_map
        .into_iter()
        .filter_map(|(inst, results)| {
            let profitable_count = results.iter().filter(|r| r.metrics.total_return > 0.0).count();
            let total = results.len();
            results.first().map(|best| InstrumentBest {
                instrument: inst,
                best_result: (*best).clone(),
                profitable_combos: profitable_count,
                total_combos: total,
            })
        })
        .collect();
    best_per_instrument.sort_by(|a, b| {
        b.best_result.composite_score
            .partial_cmp(&a.best_result.composite_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 7. Take top 50 results for the report
    let top_results: Vec<OptimizedResult> = all_results.into_iter().take(50).collect();

    let duration = start.elapsed().as_secs_f64();
    tracing::info!(
        "Optimization complete: {} combos tested, {} profitable, took {:.1}s",
        tested,
        profitable,
        duration
    );

    Ok(OptimizerReport {
        total_combinations_tested: tested,
        profitable_combinations: profitable,
        top_results,
        best_per_strategy,
        best_per_instrument,
        duration_seconds: (duration * 100.0).round() / 100.0,
        config: opt_config,
    })
}
