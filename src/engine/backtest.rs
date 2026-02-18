use crate::error::AppError;
use crate::models::{BacktestRequest, BacktestResult, BacktestMetrics, EquityPoint, TradeResponse, Candle};
use crate::engine::strategy::{StrategyDefinition, templates};
use crate::engine::indicators::{sma, ema, rsi, macd, stochastic, adx, donchian, bollinger_bands, atr};

/// Run a backtest with explicit SL/TP parameters (for custom strategies)
pub async fn run_backtest_with_params(
    symbol: &str,
    strategy_type: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: Vec<Candle>,
    initial_capital: f64,
) -> Result<BacktestResult, AppError> {
    let candles = if candles.is_empty() {
        generate_test_data(500)
    } else {
        candles
    };
    
    let ic = get_instrument_config(symbol);
    
    tracing::info!("Running backtest: {} on {} with SL={} TP={} pips on {} candles (pip={}, lot={}x{})", 
        strategy_type, symbol, sl_pips, tp_pips, candles.len(), ic.pip_value, ic.position_size, ic.lot_multiplier);
    
    // Route to appropriate strategy simulation
    let (trades, equity_curve) = match strategy_type {
        "sma_crossover" | "sma-crossover" => {
            simulate_sma_crossover(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "macd_crossover" => {
            simulate_macd_crossover(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "macd_divergence" => {
            simulate_macd_divergence(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "donchian_breakout" => {
            simulate_donchian_breakout(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "adx_trend" => {
            simulate_adx_trend(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "rsi_mean_reversion" | "rsi_reversal" | "rsi-reversal" => {
            simulate_rsi_mean_reversion(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "bollinger_mean_reversion" | "bollinger_bounce" => {
            simulate_bollinger_mean_reversion(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "stochastic_crossover" => {
            simulate_stochastic_crossover(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "ema_ribbon" => {
            simulate_ema_ribbon(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "triple_screen" => {
            simulate_triple_screen(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "london_breakout" => {
            simulate_london_breakout(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "quant_gold_momentum" => {
            simulate_quant_gold_momentum(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "rsi_reversion_2" => {
            simulate_rsi_reversion_2(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "london_ny_trend_continuation" => {
            simulate_london_ny_trend(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "london_ny_trend_2" => {
            simulate_london_ny_trend_2(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "forecast_confidence" => {
            simulate_forecast_confidence(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "volatility_expansion" => {
            simulate_volatility_expansion(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "trendline_bounce" => {
            simulate_trendline_bounce(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        "liquidity_sweep" => {
            simulate_liquidity_sweep(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
        _ => {
            // Custom/unknown strategy - use momentum entries with explicit SL/TP
            simulate_custom_strategy(&ic, symbol, sl_pips, tp_pips, &candles, initial_capital)
        }
    };
    
    let metrics = calculate_metrics(&trades, initial_capital);
    
    Ok(BacktestResult {
        id: uuid::Uuid::new_v4().to_string(),
        strategy_id: strategy_type.to_string(),
        metrics,
        equity_curve,
        trades,
        executed_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Simulate a custom strategy using momentum-based entries
fn simulate_custom_strategy(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    
    // Calculate EMA for trend and RSI for momentum
    let ema_fast = ema(candles, 8);
    let ema_slow = ema(candles, 21);
    let rsi_values = rsi(candles, 14);
    
    let warmup = 30;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        // Track equity
        let unrealized_pnl = if let Some(ref pos) = position {
            let price_diff = match pos.direction.as_str() {
                "LONG" => price - pos.entry_price,
                "SHORT" => pos.entry_price - price,
                _ => 0.0,
            };
            price_diff * ic.position_size * ic.lot_multiplier
        } else {
            0.0
        };
        
        equity_curve.push(EquityPoint {
            time: candle.time,
            equity: equity + unrealized_pnl,
        });
        
        // Check SL/TP for open position
        if let Some(ref pos) = position.clone() {
            let mut should_close = false;
            let mut exit_price = price;
            let mut exit_reason = "Signal";
            
            if let Some(sl) = pos.stop_loss {
                match pos.direction.as_str() {
                    "LONG" if candle.low <= sl => {
                        should_close = true;
                        exit_price = sl;
                        exit_reason = "Stop Loss";
                    }
                    "SHORT" if candle.high >= sl => {
                        should_close = true;
                        exit_price = sl;
                        exit_reason = "Stop Loss";
                    }
                    _ => {}
                }
            }
            
            if !should_close {
                if let Some(tp) = pos.take_profit {
                    match pos.direction.as_str() {
                        "LONG" if candle.high >= tp => {
                            should_close = true;
                            exit_price = tp;
                            exit_reason = "Take Profit";
                        }
                        "SHORT" if candle.low <= tp => {
                            should_close = true;
                            exit_price = tp;
                            exit_reason = "Take Profit";
                        }
                        _ => {}
                    }
                }
            }
            
            if should_close {
                let pnl = match pos.direction.as_str() {
                    "LONG" => (exit_price - pos.entry_price) * ic.position_size * ic.lot_multiplier,
                    "SHORT" => (pos.entry_price - exit_price) * ic.position_size * ic.lot_multiplier,
                    _ => 0.0,
                };
                
                equity += pnl;
                
                trades.push(TradeResponse {
                    id: uuid::Uuid::new_v4(),
                    strategy_id: uuid::Uuid::nil(),
                    symbol: symbol.to_string(),
                    direction: pos.direction.clone(),
                    entry_price: pos.entry_price,
                    exit_price: Some(exit_price),
                    quantity: ic.position_size,
                    pnl: Some(pnl),
                    entry_time: chrono::DateTime::from_timestamp(pos.entry_time, 0)
                        .unwrap_or_else(|| chrono::Utc::now()),
                    exit_time: Some(chrono::DateTime::from_timestamp(candle.time, 0)
                        .unwrap_or_else(|| chrono::Utc::now())),
                    status: exit_reason.to_string(),
                });
                
                position = None;
            }
        }
        
        // Entry logic: Momentum + Trend alignment
        // For scalping: look for pullbacks in trend direction
        if position.is_none() && i > 1 && i < ema_fast.len() && i < rsi_values.len() {
            let ema_f = ema_fast.get(i).copied().unwrap_or(0.0);
            let ema_s = ema_slow.get(i).copied().unwrap_or(0.0);
            let rsi_val = rsi_values.get(i).copied().unwrap_or(50.0);
            let rsi_prev = rsi_values.get(i - 1).copied().unwrap_or(50.0);
            
            // Bullish setup: EMA trending up, RSI bouncing from oversold
            let bullish = ema_f > ema_s && rsi_val > 40.0 && rsi_val < 60.0 && rsi_val > rsi_prev;
            // Bearish setup: EMA trending down, RSI bouncing from overbought  
            let bearish = ema_f < ema_s && rsi_val < 60.0 && rsi_val > 40.0 && rsi_val < rsi_prev;
            
            if bullish {
                let sl = price - sl_pips * ic.pip_value;
                let tp = price + tp_pips * ic.pip_value;
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(sl),
                    take_profit: Some(tp),
                });
            } else if bearish {
                let sl = price + sl_pips * ic.pip_value;
                let tp = price - tp_pips * ic.pip_value;
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(sl),
                    take_profit: Some(tp),
                });
            }
        }
    }
    
    // Close remaining position
    if let Some(pos) = position {
        if let Some(last_candle) = candles.last() {
            let pnl = match pos.direction.as_str() {
                "LONG" => (last_candle.close - pos.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (pos.entry_price - last_candle.close) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            };
            
            equity += pnl;
            
            trades.push(TradeResponse {
                id: uuid::Uuid::new_v4(),
                strategy_id: uuid::Uuid::nil(),
                symbol: symbol.to_string(),
                direction: pos.direction,
                entry_price: pos.entry_price,
                exit_price: Some(last_candle.close),
                quantity: ic.position_size,
                pnl: Some(pnl),
                entry_time: chrono::DateTime::from_timestamp(pos.entry_time, 0)
                    .unwrap_or_else(|| chrono::Utc::now()),
                exit_time: Some(chrono::DateTime::from_timestamp(last_candle.time, 0)
                    .unwrap_or_else(|| chrono::Utc::now())),
                status: "End of Test".to_string(),
            });
        }
    }
    
    (trades, equity_curve)
}

/// Run a backtest with a template strategy
pub async fn run_backtest(request: &BacktestRequest, candles: Vec<Candle>) -> Result<BacktestResult, AppError> {
    // Use provided candles or generate test data
    let candles = if candles.is_empty() {
        generate_test_data(500)
    } else {
        candles
    };
    
    // Get the strategy based on strategy_id or use default SMA crossover
    let strategy = match request.strategy_id.as_str() {
        "sma_crossover" | "sma-crossover" => templates::sma_crossover(9, 21),
        "rsi_reversal" | "rsi-reversal" => templates::rsi_reversal(14, 30.0, 70.0),
        "bollinger_bounce" | "bollinger-bounce" => templates::bollinger_bounce(20, 2.0),
        _ => templates::sma_crossover(9, 21), // Default
    };
    
    // Run the strategy simulation
    let ic = get_instrument_config(&request.symbol);
    let (trades, equity_curve) = simulate_strategy(&ic, &strategy, &candles, request.initial_capital);
    
    // Calculate metrics
    let metrics = calculate_metrics(&trades, request.initial_capital);
    
    Ok(BacktestResult {
        id: uuid::Uuid::new_v4().to_string(),
        strategy_id: request.strategy_id.clone(),
        metrics,
        equity_curve,
        trades,
        executed_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[derive(Clone)]
struct Position {
    entry_price: f64,
    entry_time: i64,
    direction: String,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

fn simulate_strategy(ic: &InstrumentConfig, strategy: &StrategyDefinition, candles: &[Candle], initial_capital: f64) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    // Pre-calculate indicators
    let fast_sma = sma(candles, 9);
    let slow_sma = sma(candles, 21);
    let rsi_values = rsi(candles, 14);
    
    let warmup = 50; // Minimum bars before trading
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        // Calculate unrealized P&L for equity curve
        let unrealized_pnl = if let Some(ref pos) = position {
            let price_diff = match pos.direction.as_str() {
                "LONG" => price - pos.entry_price,
                "SHORT" => pos.entry_price - price,
                _ => 0.0,
            };
            price_diff * ic.position_size * ic.lot_multiplier
        } else {
            0.0
        };
        
        equity_curve.push(EquityPoint {
            time: candle.time,
            equity: equity + unrealized_pnl,
        });
        
        // Check stop loss / take profit
        if let Some(ref pos) = position.clone() {
            let mut should_close = false;
            let mut exit_price = price;
            let mut exit_reason = "Signal";
            
            // Check SL/TP
            if let Some(sl) = pos.stop_loss {
                match pos.direction.as_str() {
                    "LONG" if candle.low <= sl => {
                        should_close = true;
                        exit_price = sl;
                        exit_reason = "Stop Loss";
                    }
                    "SHORT" if candle.high >= sl => {
                        should_close = true;
                        exit_price = sl;
                        exit_reason = "Stop Loss";
                    }
                    _ => {}
                }
            }
            
            if !should_close {
                if let Some(tp) = pos.take_profit {
                    match pos.direction.as_str() {
                        "LONG" if candle.high >= tp => {
                            should_close = true;
                            exit_price = tp;
                            exit_reason = "Take Profit";
                        }
                        "SHORT" if candle.low <= tp => {
                            should_close = true;
                            exit_price = tp;
                            exit_reason = "Take Profit";
                        }
                        _ => {}
                    }
                }
            }
            
            // Check exit signal
            if !should_close && i > 0 && i < fast_sma.len() && i < slow_sma.len() {
                let fast_prev = fast_sma.get(i - 1).copied().unwrap_or(0.0);
                let slow_prev = slow_sma.get(i - 1).copied().unwrap_or(0.0);
                let fast_curr = fast_sma.get(i).copied().unwrap_or(0.0);
                let slow_curr = slow_sma.get(i).copied().unwrap_or(0.0);
                
                match pos.direction.as_str() {
                    "LONG" if fast_prev >= slow_prev && fast_curr < slow_curr => {
                        should_close = true;
                    }
                    "SHORT" if fast_prev <= slow_prev && fast_curr > slow_curr => {
                        should_close = true;
                    }
                    _ => {}
                }
            }
            
            if should_close {
                let pnl = match pos.direction.as_str() {
                    "LONG" => (exit_price - pos.entry_price) * ic.position_size * ic.lot_multiplier,
                    "SHORT" => (pos.entry_price - exit_price) * ic.position_size * ic.lot_multiplier,
                    _ => 0.0,
                };
                
                equity += pnl;
                
                trades.push(TradeResponse {
                    id: uuid::Uuid::new_v4(),
                    strategy_id: uuid::Uuid::nil(),
                    symbol: strategy.symbol.clone(),
                    direction: pos.direction.clone(),
                    entry_price: pos.entry_price,
                    exit_price: Some(exit_price),
                    quantity: ic.position_size,
                    pnl: Some(pnl),
                    entry_time: chrono::DateTime::from_timestamp(pos.entry_time, 0)
                        .unwrap_or_else(|| chrono::Utc::now()),
                    exit_time: Some(chrono::DateTime::from_timestamp(candle.time, 0)
                        .unwrap_or_else(|| chrono::Utc::now())),
                    status: exit_reason.to_string(),
                });
                
                position = None;
            }
        }
        
        // Check entry signals (only if not in position)
        if position.is_none() && i > 0 && i < fast_sma.len() && i < slow_sma.len() {
            let fast_prev = fast_sma.get(i - 1).copied().unwrap_or(0.0);
            let slow_prev = slow_sma.get(i - 1).copied().unwrap_or(0.0);
            let fast_curr = fast_sma.get(i).copied().unwrap_or(0.0);
            let slow_curr = slow_sma.get(i).copied().unwrap_or(0.0);
            
            // Long entry: fast SMA crosses above slow SMA
            if fast_prev <= slow_prev && fast_curr > slow_curr {
                let sl = strategy.stop_loss_pips.map(|pips| price - pips * ic.pip_value);
                let tp = strategy.take_profit_pips.map(|pips| price + pips * ic.pip_value);
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: sl,
                    take_profit: tp,
                });
            }
            // Short entry: fast SMA crosses below slow SMA
            else if fast_prev >= slow_prev && fast_curr < slow_curr {
                let sl = strategy.stop_loss_pips.map(|pips| price + pips * ic.pip_value);
                let tp = strategy.take_profit_pips.map(|pips| price - pips * ic.pip_value);
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: sl,
                    take_profit: tp,
                });
            }
        }
    }
    
    // Close any remaining position at end
    if let Some(pos) = position {
        if let Some(last_candle) = candles.last() {
            let pnl = match pos.direction.as_str() {
                "LONG" => (last_candle.close - pos.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (pos.entry_price - last_candle.close) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            };
            
            equity += pnl;
            
            trades.push(TradeResponse {
                id: uuid::Uuid::new_v4(),
                strategy_id: uuid::Uuid::nil(),
                symbol: strategy.symbol.clone(),
                direction: pos.direction,
                entry_price: pos.entry_price,
                exit_price: Some(last_candle.close),
                quantity: ic.position_size,
                pnl: Some(pnl),
                entry_time: chrono::DateTime::from_timestamp(pos.entry_time, 0)
                    .unwrap_or_else(|| chrono::Utc::now()),
                exit_time: Some(chrono::DateTime::from_timestamp(last_candle.time, 0)
                    .unwrap_or_else(|| chrono::Utc::now())),
                status: "End of Test".to_string(),
            });
        }
    }
    
    (trades, equity_curve)
}

fn calculate_metrics(trades: &[TradeResponse], initial_capital: f64) -> BacktestMetrics {
    if trades.is_empty() {
        return BacktestMetrics {
            total_return: 0.0,
            sharpe_ratio: 0.0,
            max_drawdown: 0.0,
            win_rate: 0.0,
            total_trades: 0,
            profit_factor: 0.0,
            average_win: 0.0,
            average_loss: 0.0,
        };
    }
    
    let winning_trades: Vec<_> = trades.iter()
        .filter(|t| t.pnl.unwrap_or(0.0) > 0.0)
        .collect();
    let losing_trades: Vec<_> = trades.iter()
        .filter(|t| t.pnl.unwrap_or(0.0) < 0.0)
        .collect();
    
    let total_profit: f64 = winning_trades.iter()
        .map(|t| t.pnl.unwrap_or(0.0))
        .sum();
    let total_loss: f64 = losing_trades.iter()
        .map(|t| t.pnl.unwrap_or(0.0).abs())
        .sum();
    
    let final_equity: f64 = initial_capital + trades.iter()
        .map(|t| t.pnl.unwrap_or(0.0))
        .sum::<f64>();
    
    let total_return = ((final_equity - initial_capital) / initial_capital) * 100.0;
    let win_rate = (winning_trades.len() as f64 / trades.len() as f64) * 100.0;
    let profit_factor = if total_loss > 0.0 { total_profit / total_loss } else { 0.0 };
    let average_win = if !winning_trades.is_empty() {
        total_profit / winning_trades.len() as f64
    } else {
        0.0
    };
    let average_loss = if !losing_trades.is_empty() {
        total_loss / losing_trades.len() as f64
    } else {
        0.0
    };
    
    // Simplified Sharpe ratio calculation
    let returns: Vec<f64> = trades.iter()
        .map(|t| t.pnl.unwrap_or(0.0) / initial_capital)
        .collect();
    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance: f64 = returns.iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>() / returns.len() as f64;
    let std_dev = variance.sqrt();
    let sharpe_ratio = if std_dev > 0.0 { mean_return / std_dev * (252_f64).sqrt() } else { 0.0 };
    
    // Phase 5: Calculate actual max drawdown from equity curve
    let max_drawdown = calculate_max_drawdown(trades, initial_capital);
    
    BacktestMetrics {
        total_return: (total_return * 100.0).round() / 100.0,
        sharpe_ratio: (sharpe_ratio * 100.0).round() / 100.0,
        max_drawdown: (max_drawdown * 100.0).round() / 100.0,
        win_rate: (win_rate * 100.0).round() / 100.0,
        total_trades: trades.len(),
        profit_factor: (profit_factor * 100.0).round() / 100.0,
        average_win: (average_win * 100.0).round() / 100.0,
        average_loss: (average_loss * 100.0).round() / 100.0,
    }
}

/// Calculate maximum drawdown from trade history
fn calculate_max_drawdown(trades: &[TradeResponse], initial_capital: f64) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }
    
    let mut equity = initial_capital;
    let mut peak_equity = initial_capital;
    let mut max_dd = 0.0;
    
    for trade in trades {
        equity += trade.pnl.unwrap_or(0.0);
        
        // Update peak
        if equity > peak_equity {
            peak_equity = equity;
        }
        
        // Calculate drawdown from peak
        let dd_pct = if peak_equity > 0.0 {
            ((peak_equity - equity) / peak_equity) * 100.0
        } else {
            0.0
        };
        
        // Track maximum drawdown
        if dd_pct > max_dd {
            max_dd = dd_pct;
        }
    }
    
    max_dd
}

fn generate_test_data(num_candles: usize) -> Vec<Candle> {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    let mut candles = Vec::new();
    let mut price = 2040.0;
    
    for i in 0..num_candles {
        let time = now - ((num_candles - i) as i64 * 4 * 3600);
        let change = (i as f64 * 0.1).sin() * 5.0 + (i as f64 * 0.05).cos() * 3.0;
        price += change;
        
        let open = price;
        let close = price + (i as f64 * 0.07).sin() * 2.0;
        let high = open.max(close) + 1.5;
        let low = open.min(close) - 1.5;
        
        candles.push(Candle {
            time,
            open,
            high,
            low,
            close,
            volume: Some(1000.0 + (i as f64 * 100.0)),
        });
    }
    
    candles
}

// ============================================================================
// INSTRUMENT CONFIGURATION
// ============================================================================

/// Per-instrument trading parameters for accurate backtesting.
#[derive(Debug, Clone)]
pub struct InstrumentConfig {
    pub pip_value: f64,         // Smallest price increment (0.1 for gold, 0.0001 for EUR/USD)
    pub position_size: f64,     // Default lot size
    pub lot_multiplier: f64,    // Units per lot (100 for gold, 100000 for FX)
    pub slippage_pips: f64,     // Average slippage per side in pips
    pub commission_per_lot: f64, // Round-trip commission per standard lot
}

/// Look up instrument-specific trading parameters.
pub fn get_instrument_config(symbol: &str) -> InstrumentConfig {
    // Normalize: "XAUUSD" -> "XAU_USD", handle both formats
    let sym = symbol.to_uppercase().replace('_', "");
    match sym.as_str() {
        // Precious metals
        "XAUUSD" => InstrumentConfig {
            pip_value: 0.1,           // $0.10 per pip
            position_size: 0.1,       // 0.1 lots = 10 oz
            lot_multiplier: 100.0,    // 1 lot = 100 troy oz
            slippage_pips: 0.5,
            commission_per_lot: 7.0,
        },
        // Crypto
        "BTCUSD" => InstrumentConfig {
            pip_value: 1.0,           // $1.00 per pip
            position_size: 0.01,      // 0.01 BTC
            lot_multiplier: 1.0,      // 1 lot = 1 BTC
            slippage_pips: 5.0,
            commission_per_lot: 0.0,   // Spread-based
        },
        // Major FX pairs
        "EURUSD" | "GBPUSD" | "AUDUSD" | "NZDUSD" => InstrumentConfig {
            pip_value: 0.0001,
            position_size: 0.1,       // 0.1 lots = 10,000 units
            lot_multiplier: 100_000.0,
            slippage_pips: 1.0,
            commission_per_lot: 5.0,
        },
        // JPY pairs
        "USDJPY" | "EURJPY" | "GBPJPY" => InstrumentConfig {
            pip_value: 0.01,
            position_size: 0.1,
            lot_multiplier: 100_000.0,
            slippage_pips: 1.0,
            commission_per_lot: 5.0,
        },
        // Oil
        "WTICOUSD" | "BCOUSD" => InstrumentConfig {
            pip_value: 0.01,
            position_size: 0.1,
            lot_multiplier: 1_000.0,   // 1 lot = 1000 barrels
            slippage_pips: 2.0,
            commission_per_lot: 5.0,
        },
        // Natural Gas
        "NATGASUSD" => InstrumentConfig {
            pip_value: 0.001,
            position_size: 0.1,
            lot_multiplier: 10_000.0,
            slippage_pips: 3.0,
            commission_per_lot: 5.0,
        },
        // US Indices
        "SPX500USD" | "NAS100USD" | "US30USD" => InstrumentConfig {
            pip_value: 0.1,
            position_size: 1.0,
            lot_multiplier: 1.0,       // 1 lot = 1 unit (CFD)
            slippage_pips: 1.0,
            commission_per_lot: 3.0,
        },
        // Default: treat like gold
        _ => {
            tracing::warn!("Unknown instrument '{}', using gold defaults", symbol);
            InstrumentConfig {
                pip_value: 0.1,
                position_size: 0.1,
                lot_multiplier: 100.0,
                slippage_pips: 0.5,
                commission_per_lot: 7.0,
            }
        }
    }
}

// ============================================================================
// STRATEGY IMPLEMENTATIONS
// ============================================================================

/// Helper to create a trade response with slippage and commission
fn create_trade(ic: &InstrumentConfig, 
    symbol: &str,
    pos: &Position,
    exit_price: f64,
    exit_time: i64,
    exit_reason: &str,
) -> (f64, TradeResponse) {
    // Apply slippage: unfavorable fills on both entry and exit
    let slippage_cost = ic.slippage_pips * ic.pip_value * 2.0; // Entry + Exit
    let slippage_amount = slippage_cost * ic.position_size * ic.lot_multiplier;
    
    // Calculate base P&L
    let base_pnl = match pos.direction.as_str() {
        "LONG" => (exit_price - pos.entry_price) * ic.position_size * ic.lot_multiplier,
        "SHORT" => (pos.entry_price - exit_price) * ic.position_size * ic.lot_multiplier,
        _ => 0.0,
    };
    
    // Apply costs: slippage + commission
    let commission = ic.commission_per_lot * ic.position_size;
    let pnl = base_pnl - slippage_amount - commission;
    
    let trade = TradeResponse {
        id: uuid::Uuid::new_v4(),
        strategy_id: uuid::Uuid::nil(),
        symbol: symbol.to_string(),
        direction: pos.direction.clone(),
        entry_price: pos.entry_price,
        exit_price: Some(exit_price),
        quantity: ic.position_size,
        pnl: Some(pnl),
        entry_time: chrono::DateTime::from_timestamp(pos.entry_time, 0)
            .unwrap_or_else(|| chrono::Utc::now()),
        exit_time: Some(chrono::DateTime::from_timestamp(exit_time, 0)
            .unwrap_or_else(|| chrono::Utc::now())),
        status: exit_reason.to_string(),
    };
    
    (pnl, trade)
}

/// Check SL/TP and return exit info if triggered
fn check_sl_tp(pos: &Position, candle: &Candle) -> Option<(f64, &'static str)> {
    // Check stop loss first
    if let Some(sl) = pos.stop_loss {
        match pos.direction.as_str() {
            "LONG" if candle.low <= sl => return Some((sl, "Stop Loss")),
            "SHORT" if candle.high >= sl => return Some((sl, "Stop Loss")),
            _ => {}
        }
    }
    
    // Check take profit
    if let Some(tp) = pos.take_profit {
        match pos.direction.as_str() {
            "LONG" if candle.high >= tp => return Some((tp, "Take Profit")),
            "SHORT" if candle.low <= tp => return Some((tp, "Take Profit")),
            _ => {}
        }
    }
    
    None
}

/// SMA Crossover Strategy
fn simulate_sma_crossover(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let fast_sma = sma(candles, 9);
    let slow_sma = sma(candles, 21);
    let warmup = 25;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        // Update equity curve
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        // Check exits
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        // Check entries
        if position.is_none() && i > 0 {
            let fast_prev = fast_sma.get(i - 1).copied().unwrap_or(0.0);
            let slow_prev = slow_sma.get(i - 1).copied().unwrap_or(0.0);
            let fast_curr = fast_sma.get(i).copied().unwrap_or(0.0);
            let slow_curr = slow_sma.get(i).copied().unwrap_or(0.0);
            
            // Golden cross - long
            if fast_prev <= slow_prev && fast_curr > slow_curr {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Death cross - short
            else if fast_prev >= slow_prev && fast_curr < slow_curr {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    // Close remaining position
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// MACD Crossover Strategy
fn simulate_macd_crossover(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (macd_line, signal_line, _histogram) = macd(candles, 12, 26, 9);
    let warmup = 35;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i > 0 {
            let macd_prev = macd_line.get(i - 1).copied().unwrap_or(0.0);
            let signal_prev = signal_line.get(i - 1).copied().unwrap_or(0.0);
            let macd_curr = macd_line.get(i).copied().unwrap_or(0.0);
            let signal_curr = signal_line.get(i).copied().unwrap_or(0.0);
            
            // MACD crosses above signal - bullish
            if macd_prev <= signal_prev && macd_curr > signal_curr {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // MACD crosses below signal - bearish
            else if macd_prev >= signal_prev && macd_curr < signal_curr {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// MACD Divergence Strategy (simplified)
fn simulate_macd_divergence(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (_macd_line, _signal_line, histogram) = macd(candles, 12, 26, 9);
    let warmup = 40;
    let lookback = 10;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i >= lookback {
            // Find local lows/highs in price and histogram
            let price_slice: Vec<f64> = candles[i-lookback..i].iter().map(|c| c.low).collect();
            let hist_slice: Vec<f64> = histogram[i-lookback..i].to_vec();
            
            let price_min = price_slice.iter().cloned().fold(f64::INFINITY, f64::min);
            let hist_min = hist_slice.iter().cloned().fold(f64::INFINITY, f64::min);
            
            // Bullish divergence: price making lower lows, histogram making higher lows
            if candle.low < price_min && histogram.get(i).copied().unwrap_or(0.0) > hist_min && hist_min < 0.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            
            let price_slice_high: Vec<f64> = candles[i-lookback..i].iter().map(|c| c.high).collect();
            let price_max = price_slice_high.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let hist_max = hist_slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            
            // Bearish divergence: price making higher highs, histogram making lower highs
            if position.is_none() && candle.high > price_max && histogram.get(i).copied().unwrap_or(0.0) < hist_max && hist_max > 0.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// Donchian Channel Breakout Strategy (Turtle Trading)
fn simulate_donchian_breakout(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (upper, _middle, lower) = donchian(candles, 20);
    let warmup = 25;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i > 0 {
            let prev_upper = upper.get(i - 1).copied().unwrap_or(0.0);
            let prev_lower = lower.get(i - 1).copied().unwrap_or(f64::MAX);
            
            // Breakout above upper channel - long
            if candle.high > prev_upper && prev_upper > 0.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Breakout below lower channel - short
            else if candle.low < prev_lower && prev_lower < f64::MAX {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// ADX Trend Strategy
fn simulate_adx_trend(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    let warmup = 30;
    let adx_threshold = 25.0;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() {
            let adx_val = adx_values.get(i).copied().unwrap_or(0.0);
            let plus = plus_di.get(i).copied().unwrap_or(0.0);
            let minus = minus_di.get(i).copied().unwrap_or(0.0);
            
            // Only trade when ADX > threshold (strong trend)
            if adx_val > adx_threshold {
                // +DI > -DI = bullish trend
                if plus > minus {
                    position = Some(Position {
                        entry_price: price,
                        entry_time: candle.time,
                        direction: "LONG".to_string(),
                        stop_loss: Some(price - sl_pips * ic.pip_value),
                        take_profit: Some(price + tp_pips * ic.pip_value),
                    });
                }
                // -DI > +DI = bearish trend
                else if minus > plus {
                    position = Some(Position {
                        entry_price: price,
                        entry_time: candle.time,
                        direction: "SHORT".to_string(),
                        stop_loss: Some(price + sl_pips * ic.pip_value),
                        take_profit: Some(price - tp_pips * ic.pip_value),
                    });
                }
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// RSI Mean Reversion Strategy
fn simulate_rsi_mean_reversion(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let rsi_values = rsi(candles, 14);
    let warmup = 20;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() {
            let rsi_val = rsi_values.get(i).copied().unwrap_or(50.0);
            
            // RSI oversold - buy
            if rsi_val < 30.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // RSI overbought - sell
            else if rsi_val > 70.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// Bollinger Band Mean Reversion Strategy
fn simulate_bollinger_mean_reversion(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (bb_upper, _bb_middle, bb_lower) = bollinger_bands(candles, 20, 2.0);
    let warmup = 25;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i < bb_lower.len() && i < bb_upper.len() {
            let lower = bb_lower[i];
            let upper = bb_upper[i];
            
            // Price touches lower band - buy (mean reversion)
            if candle.low <= lower {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Price touches upper band - sell (mean reversion)
            else if candle.high >= upper {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// Stochastic Crossover Strategy
fn simulate_stochastic_crossover(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let (k_values, d_values) = stochastic(candles, 14, 3);
    let warmup = 20;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i > 0 {
            let k_prev = k_values.get(i - 1).copied().unwrap_or(50.0);
            let d_prev = d_values.get(i - 1).copied().unwrap_or(50.0);
            let k_curr = k_values.get(i).copied().unwrap_or(50.0);
            let d_curr = d_values.get(i).copied().unwrap_or(50.0);
            
            // %K crosses above %D in oversold zone
            if k_prev <= d_prev && k_curr > d_curr && k_curr < 20.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // %K crosses below %D in overbought zone
            else if k_prev >= d_prev && k_curr < d_curr && k_curr > 80.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// EMA Ribbon Strategy
fn simulate_ema_ribbon(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let ema8 = ema(candles, 8);
    let ema13 = ema(candles, 13);
    let ema21 = ema(candles, 21);
    let ema34 = ema(candles, 34);
    let ema55 = ema(candles, 55);
    let warmup = 60;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() {
            let e8 = ema8.get(i).copied().unwrap_or(0.0);
            let e13 = ema13.get(i).copied().unwrap_or(0.0);
            let e21 = ema21.get(i).copied().unwrap_or(0.0);
            let e34 = ema34.get(i).copied().unwrap_or(0.0);
            let e55 = ema55.get(i).copied().unwrap_or(0.0);
            
            // All EMAs aligned bullish (8 > 13 > 21 > 34 > 55)
            if e8 > e13 && e13 > e21 && e21 > e34 && e34 > e55 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // All EMAs aligned bearish (8 < 13 < 21 < 34 < 55)
            else if e8 < e13 && e13 < e21 && e21 < e34 && e34 < e55 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// Triple Screen Strategy (simplified - uses EMA trend + Stochastic timing)
fn simulate_triple_screen(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    let ema13 = ema(candles, 13);
    let (k_values, d_values) = stochastic(candles, 5, 3);
    let warmup = 20;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() && i > 1 {
            let ema_curr = ema13.get(i).copied().unwrap_or(0.0);
            let ema_prev = ema13.get(i - 1).copied().unwrap_or(0.0);
            let k_curr = k_values.get(i).copied().unwrap_or(50.0);
            let d_curr = d_values.get(i).copied().unwrap_or(50.0);
            let k_prev = k_values.get(i - 1).copied().unwrap_or(50.0);
            let d_prev = d_values.get(i - 1).copied().unwrap_or(50.0);
            
            // Screen 1: EMA trend up + Screen 2: Stochastic bullish cross in oversold
            if ema_curr > ema_prev && k_prev <= d_prev && k_curr > d_curr && k_curr < 30.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Screen 1: EMA trend down + Screen 2: Stochastic bearish cross in overbought
            else if ema_curr < ema_prev && k_prev >= d_prev && k_curr < d_curr && k_curr > 70.0 {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// London Session Breakout Strategy
fn simulate_london_breakout(ic: &InstrumentConfig, 
    symbol: &str,
    sl_pips: f64,
    tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    // Track Asian session range (simplified using recent range)
    let range_period = 12; // Approx 3 hours of M15 candles
    let warmup = range_period + 5;
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        if position.is_none() {
            // Calculate range from previous candles
            let range_high = candles[i-range_period..i].iter()
                .map(|c| c.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let range_low = candles[i-range_period..i].iter()
                .map(|c| c.low)
                .fold(f64::INFINITY, f64::min);
            
            // Breakout above range
            if candle.close > range_high {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Breakout below range
            else if candle.close < range_low {
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }
    
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

/// Quantitative Gold Momentum Reversion Strategy
/// 
/// Statistical approach for XAUUSD:
/// - 200 EMA trend filter
/// - RSI pullback entries (35/65 levels)
/// - Value zone filter (within 1.5 ATR of 21 EMA)
/// - Momentum confirmation (RSI turning)
fn simulate_quant_gold_momentum(ic: &InstrumentConfig, 
    symbol: &str,
    _sl_pips: f64,  // We use ATR-based SL/TP instead
    _tp_pips: f64,
    candles: &[Candle],
    initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;
    
    // Calculate indicators
    let ema_200 = ema(candles, 200);
    let ema_21 = ema(candles, 21);
    let rsi_values = rsi(candles, 14);
    let atr_values = atr(candles, 14);
    
    // Strategy parameters
    let rsi_oversold = 35.0;
    let rsi_overbought = 65.0;
    let atr_distance_mult = 1.5;
    let sl_atr_mult = 1.5;  // Stop loss at 1.5 ATR
    let tp_atr_mult = 2.0;  // Take profit at 2.0 ATR (1:1.33 R:R)
    
    let warmup = 210; // Need 200+ candles for EMA(200)
    
    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;
        
        // Track equity
        let unrealized = position.as_ref().map(|p| {
            match p.direction.as_str() {
                "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
                "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
                _ => 0.0,
            }
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });
        
        // Check SL/TP
        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }
        
        // Entry logic
        if position.is_none() && i >= 2 {
            let ema200 = ema_200.get(i).copied().unwrap_or(0.0);
            let ema21 = ema_21.get(i).copied().unwrap_or(0.0);
            let rsi_curr = rsi_values.get(i).copied().unwrap_or(50.0);
            let rsi_prev = rsi_values.get(i - 1).copied().unwrap_or(50.0);
            let rsi_prev2 = rsi_values.get(i - 2).copied().unwrap_or(50.0);
            let atr_val = atr_values.get(i).copied().unwrap_or(1.0);
            
            if atr_val == 0.0 {
                continue;
            }
            
            // Trend filter
            let is_uptrend = price > ema200;
            
            // Value zone filter
            let distance_from_ema21 = (price - ema21).abs();
            let is_in_value_zone = distance_from_ema21 <= atr_val * atr_distance_mult;
            
            // Momentum filter
            let rsi_turning_up = rsi_curr > rsi_prev && rsi_prev <= rsi_prev2;
            let rsi_turning_down = rsi_curr < rsi_prev && rsi_prev >= rsi_prev2;
            
            // LONG: Uptrend + RSI oversold + in value zone + RSI turning up
            if is_uptrend && rsi_curr < rsi_oversold && is_in_value_zone && rsi_turning_up {
                let sl = price - atr_val * sl_atr_mult;
                let tp = price + atr_val * tp_atr_mult;
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(sl),
                    take_profit: Some(tp),
                });
            }
            // SHORT: Downtrend + RSI overbought + in value zone + RSI turning down
            else if !is_uptrend && rsi_curr > rsi_overbought && is_in_value_zone && rsi_turning_down {
                let sl = price + atr_val * sl_atr_mult;
                let tp = price - atr_val * tp_atr_mult;
                
                position = Some(Position {
                    entry_price: price,
                    entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(sl),
                    take_profit: Some(tp),
                });
            }
        }
    }
    
    // Close any remaining position
    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl;
            trades.push(trade);
        }
    }
    
    (trades, equity_curve)
}

// ============================================================================
// NEW STRATEGIES
// ============================================================================

/// RSI Reversion 2.0 - Enhanced RSI with EMA200 trend + ADX + ATR volatility filter
fn simulate_rsi_reversion_2(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let ema_200 = ema(candles, 200);
    let rsi_values = rsi(candles, 14);
    let atr_values = atr(candles, 14);
    let (adx_values, _plus_di, _minus_di) = adx(candles, 14);
    let warmup = 210;

    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl;
                trades.push(trade);
                position = None;
            }
        }

        if position.is_none() && i >= 2 {
            let ema200 = ema_200.get(i).copied().unwrap_or(0.0);
            let rsi_curr = rsi_values.get(i).copied().unwrap_or(50.0);
            let rsi_prev = rsi_values.get(i - 1).copied().unwrap_or(50.0);
            let atr_val = atr_values.get(i).copied().unwrap_or(1.0);
            let adx_val = adx_values.get(i).copied().unwrap_or(0.0);

            if atr_val == 0.0 { continue; }

            // ADX trending filter
            let is_trending = adx_val > 20.0;
            // ATR volatility filter: skip abnormal volatility
            let atr_ma: f64 = if i >= 50 {
                atr_values[i.saturating_sub(50)..i].iter().sum::<f64>() / 50.0
            } else { atr_val };
            let vol_ratio = atr_val / atr_ma.max(0.0001);
            let normal_vol = vol_ratio >= 0.5 && vol_ratio <= 2.0;

            let is_uptrend = price > ema200;
            let rsi_turning_up = rsi_curr > rsi_prev;
            let rsi_turning_down = rsi_curr < rsi_prev;

            if is_trending && normal_vol {
                // LONG: uptrend + RSI oversold + turning up
                if is_uptrend && rsi_curr < 30.0 && rsi_turning_up {
                    position = Some(Position {
                        entry_price: price, entry_time: candle.time,
                        direction: "LONG".to_string(),
                        stop_loss: Some(price - sl_pips * ic.pip_value),
                        take_profit: Some(price + tp_pips * ic.pip_value),
                    });
                }
                // SHORT: downtrend + RSI overbought + turning down
                else if !is_uptrend && rsi_curr > 70.0 && rsi_turning_down {
                    position = Some(Position {
                        entry_price: price, entry_time: candle.time,
                        direction: "SHORT".to_string(),
                        stop_loss: Some(price + sl_pips * ic.pip_value),
                        take_profit: Some(price - tp_pips * ic.pip_value),
                    });
                }
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// London/NY Trend Continuation - Session + EMA200 trend + ADX + pullback zone + RSI cross
fn simulate_london_ny_trend(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let ema_200 = ema(candles, 200);
    let ema_21 = ema(candles, 21);
    let ema_50 = ema(candles, 50);
    let rsi_values = rsi(candles, 14);
    let atr_values = atr(candles, 14);
    let (adx_values, _plus, _minus) = adx(candles, 14);
    let warmup = 210;

    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        if position.is_none() && i > 1 {
            // Session filter: use candle timestamp (not live clock)
            let hour = ((candle.time % 86400) / 3600) as u32;
            let in_session = hour >= 7 && hour < 16;
            if !in_session { continue; }

            let ema200 = ema_200.get(i).copied().unwrap_or(0.0);
            let ema21 = ema_21.get(i).copied().unwrap_or(0.0);
            let ema50 = ema_50.get(i).copied().unwrap_or(0.0);
            let rsi_curr = rsi_values.get(i).copied().unwrap_or(50.0);
            let rsi_prev = rsi_values.get(i - 1).copied().unwrap_or(50.0);
            let atr_val = atr_values.get(i).copied().unwrap_or(1.0);
            let adx_val = adx_values.get(i).copied().unwrap_or(0.0);

            let is_uptrend = price > ema200;
            let is_trending = adx_val > 20.0;

            // Pullback zone: price between EMA21 and EMA50, or within 1.2 ATR of EMA21
            let in_pullback = (price >= ema50.min(ema21) && price <= ema50.max(ema21))
                || (price - ema21).abs() <= atr_val * 1.2;

            // RSI cross triggers
            let rsi_long_cross = rsi_prev < 45.0 && rsi_curr >= 45.0;
            let rsi_short_cross = rsi_prev > 55.0 && rsi_curr <= 55.0;

            // Candle confirmation
            let bullish_candle = candle.close > candle.open && candle.close > candles[i-1].high;
            let bearish_candle = candle.close < candle.open && candle.close < candles[i-1].low;

            if is_uptrend && is_trending && in_pullback && rsi_long_cross && bullish_candle {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            } else if !is_uptrend && is_trending && in_pullback && rsi_short_cross && bearish_candle {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// London/NY Trend 2.0 - Same as above + MACD histogram confirmation
fn simulate_london_ny_trend_2(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let ema_200 = ema(candles, 200);
    let ema_21 = ema(candles, 21);
    let ema_50 = ema(candles, 50);
    let rsi_values = rsi(candles, 14);
    let atr_values = atr(candles, 14);
    let (adx_values, _plus, _minus) = adx(candles, 14);
    let (_macd_line, _signal_line, histogram) = macd(candles, 12, 26, 9);
    let warmup = 210;

    for i in warmup..candles.len() {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        if position.is_none() && i > 1 {
            let hour = ((candle.time % 86400) / 3600) as u32;
            if !(hour >= 7 && hour < 16) { continue; }

            let ema200 = ema_200.get(i).copied().unwrap_or(0.0);
            let ema21 = ema_21.get(i).copied().unwrap_or(0.0);
            let ema50 = ema_50.get(i).copied().unwrap_or(0.0);
            let rsi_curr = rsi_values.get(i).copied().unwrap_or(50.0);
            let rsi_prev = rsi_values.get(i - 1).copied().unwrap_or(50.0);
            let atr_val = atr_values.get(i).copied().unwrap_or(1.0);
            let adx_val = adx_values.get(i).copied().unwrap_or(0.0);
            let macd_hist = histogram.get(i).copied().unwrap_or(0.0);
            let macd_hist_prev = histogram.get(i - 1).copied().unwrap_or(0.0);

            let is_uptrend = price > ema200;
            let is_trending = adx_val > 20.0;
            let in_pullback = (price >= ema50.min(ema21) && price <= ema50.max(ema21))
                || (price - ema21).abs() <= atr_val * 1.5;

            let rsi_long_cross = rsi_prev < 45.0 && rsi_curr >= 45.0;
            let rsi_short_cross = rsi_prev > 55.0 && rsi_curr <= 55.0;
            let bullish_candle = candle.close > candle.open && candle.close > candles[i-1].high;
            let bearish_candle = candle.close < candle.open && candle.close < candles[i-1].low;

            // MACD histogram confirmation
            let macd_bullish = macd_hist > 0.0 || (macd_hist > macd_hist_prev && macd_hist_prev < 0.0);
            let macd_bearish = macd_hist < 0.0 || (macd_hist < macd_hist_prev && macd_hist_prev > 0.0);

            if is_uptrend && is_trending && in_pullback && rsi_long_cross && bullish_candle && macd_bullish {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            } else if !is_uptrend && is_trending && in_pullback && rsi_short_cross && bearish_candle && macd_bearish {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// Forecast Confidence - Holt-Winters exponential smoothing + rolling accuracy + RSI agreement
fn simulate_forecast_confidence(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let alpha = 0.3;
    let beta = 0.1;
    let forecast_threshold = 0.3; // % edge required
    let accuracy_window = 30;
    let min_accuracy = 55.0;
    let rsi_values = rsi(candles, 14);
    let n = candles.len();
    let warmup = accuracy_window + 35;

    if n < warmup + 5 {
        return (trades, equity_curve);
    }

    // Run Holt-Winters up to warmup to initialize
    let mut level = candles[0].close;
    let mut trend = (candles[4.min(n-1)].close - candles[0].close) / 4.0;

    for j in 1..warmup {
        let y = candles[j].close;
        let prev_level = level;
        level = alpha * y + (1.0 - alpha) * (level + trend);
        trend = beta * (level - prev_level) + (1.0 - beta) * trend;
    }

    for i in warmup..n {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        // Forecast for next bar
        let predicted_price = level + trend;
        let forecast_edge = ((predicted_price - price) / price) * 100.0;

        // Rolling accuracy: check last accuracy_window predictions
        let mut correct = 0;
        let mut total = 0;
        let acc_start = i.saturating_sub(accuracy_window);
        if acc_start > 0 {
            let mut h_level = candles[0].close;
            let mut h_trend = (candles[4.min(n-1)].close - candles[0].close) / 4.0;
            for j in 1..acc_start {
                let y = candles[j].close;
                let pl = h_level;
                h_level = alpha * y + (1.0 - alpha) * (h_level + h_trend);
                h_trend = beta * (h_level - pl) + (1.0 - beta) * h_trend;
            }
            for j in acc_start..i {
                let forecast = h_level + h_trend;
                let actual_prev = candles[j.saturating_sub(1)].close;
                let actual_curr = candles[j].close;
                if (forecast > actual_prev) == (actual_curr > actual_prev) {
                    correct += 1;
                }
                total += 1;
                let pl = h_level;
                h_level = alpha * actual_curr + (1.0 - alpha) * (h_level + h_trend);
                h_trend = beta * (h_level - pl) + (1.0 - beta) * h_trend;
            }
        }

        let accuracy = if total > 0 { (correct as f64 / total as f64) * 100.0 } else { 0.0 };
        let rsi_curr = rsi_values.get(i).copied().unwrap_or(50.0);

        // Update HW model
        let prev_level = level;
        level = alpha * price + (1.0 - alpha) * (level + trend);
        trend = beta * (level - prev_level) + (1.0 - beta) * trend;

        if position.is_none() && accuracy >= min_accuracy {
            let bullish = forecast_edge > forecast_threshold;
            let bearish = forecast_edge < -forecast_threshold;
            let rsi_agrees_long = bullish && rsi_curr > 40.0;
            let rsi_agrees_short = bearish && rsi_curr < 60.0;

            if rsi_agrees_long {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            } else if rsi_agrees_short {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// Volatility Expansion - Vol compression + BB squeeze + Donchian breakout
fn simulate_volatility_expansion(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let (bb_upper, bb_middle, bb_lower) = bollinger_bands(candles, 20, 2.0);
    let (dc_upper, dc_lower, _) = donchian(candles, 20);
    let n = candles.len();
    let warmup = 130; // Need 120+ for bandwidth history

    for i in warmup..n {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        if position.is_none() && i > 0 {
            // Realized volatility: 20d vs 90d
            let calc_vol = |start: usize, end: usize| -> f64 {
                if end <= start + 1 { return 0.0; }
                let returns: Vec<f64> = (start+1..=end).filter(|&j| j < n)
                    .map(|j| (candles[j].close / candles[j-1].close).ln()).collect();
                if returns.is_empty() { return 0.0; }
                let mean = returns.iter().sum::<f64>() / returns.len() as f64;
                let var: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
                var.sqrt() * (252.0_f64).sqrt() * 100.0
            };
            let vol_20 = calc_vol(i.saturating_sub(20), i);
            let vol_90 = calc_vol(i.saturating_sub(90), i);
            let vol_compressed = vol_90 > 0.0 && vol_20 / vol_90 < 0.6;

            // BB bandwidth squeeze
            let current_bw = if i < bb_upper.len() && bb_middle[i] > 0.0 {
                (bb_upper[i] - bb_lower[i]) / bb_middle[i]
            } else { 0.0 };
            let min_bw = (i.saturating_sub(120)..i)
                .filter(|&j| j < bb_upper.len() && bb_middle[j] > 0.0)
                .map(|j| (bb_upper[j] - bb_lower[j]) / bb_middle[j])
                .fold(f64::INFINITY, f64::min);
            let bw_at_low = current_bw <= min_bw * 1.05;

            let squeeze = vol_compressed && bw_at_low;

            // Donchian breakout
            let prev_dc_high = dc_upper.get(i - 1).copied().unwrap_or(price);
            let prev_dc_low = dc_lower.get(i - 1).copied().unwrap_or(price);

            if squeeze && price > prev_dc_high {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            } else if squeeze && price < prev_dc_low {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// Trendline Bounce - Swing detection + linear regression trendline + pullback + bounce
fn simulate_trendline_bounce(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let (adx_values, _plus, _minus) = adx(candles, 14);
    let atr_values = atr(candles, 14);
    let n = candles.len();
    let swing_lookback = 5;
    let warmup = 50;

    fn linreg(points: &[(usize, f64)]) -> Option<(f64, f64)> {
        if points.len() < 2 { return None; }
        let n = points.len() as f64;
        let sx: f64 = points.iter().map(|(x,_)| *x as f64).sum();
        let sy: f64 = points.iter().map(|(_,y)| *y).sum();
        let sxy: f64 = points.iter().map(|(x,y)| *x as f64 * y).sum();
        let sxx: f64 = points.iter().map(|(x,_)| (*x as f64).powi(2)).sum();
        let d = n * sxx - sx.powi(2);
        if d.abs() < 1e-10 { return None; }
        Some(((n * sxy - sx * sy) / d, (sy - ((n * sxy - sx * sy) / d) * sx) / n))
    }

    for i in warmup..n {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        if position.is_none() && i > swing_lookback + 1 {
            let adx_val = adx_values.get(i).copied().unwrap_or(0.0);
            let atr_val = atr_values.get(i).copied().unwrap_or(1.0);
            if adx_val < 20.0 || atr_val == 0.0 { continue; }

            // Detect recent swing lows/highs
            let recent_start = i.saturating_sub(50);
            let mut swing_lows: Vec<(usize, f64)> = Vec::new();
            let mut swing_highs: Vec<(usize, f64)> = Vec::new();
            for j in (recent_start + swing_lookback)..(i.saturating_sub(swing_lookback)) {
                if j >= n { break; }
                let is_low = (j.saturating_sub(swing_lookback)..j).all(|k| candles[k].low >= candles[j].low)
                    && (j+1..=(j+swing_lookback).min(n-1)).all(|k| candles[k].low >= candles[j].low);
                let is_high = (j.saturating_sub(swing_lookback)..j).all(|k| candles[k].high <= candles[j].high)
                    && (j+1..=(j+swing_lookback).min(n-1)).all(|k| candles[k].high <= candles[j].high);
                if is_low { swing_lows.push((j, candles[j].low)); }
                if is_high { swing_highs.push((j, candles[j].high)); }
            }

            // Uptrend trendline from swing lows
            if let Some((slope, intercept)) = linreg(&swing_lows) {
                if slope > 0.05 {
                    let tl_price = slope * i as f64 + intercept;
                    let touch_dist = (candle.low - tl_price).abs();
                    if touch_dist <= atr_val * 0.5 && candle.close > candle.open {
                        position = Some(Position {
                            entry_price: price, entry_time: candle.time,
                            direction: "LONG".to_string(),
                            stop_loss: Some(price - sl_pips * ic.pip_value),
                            take_profit: Some(price + tp_pips * ic.pip_value),
                        });
                    }
                }
            }
            // Downtrend trendline from swing highs
            if position.is_none() {
                if let Some((slope, intercept)) = linreg(&swing_highs) {
                    if slope < -0.05 {
                        let tl_price = slope * i as f64 + intercept;
                        let touch_dist = (candle.high - tl_price).abs();
                        if touch_dist <= atr_val * 0.5 && candle.close < candle.open {
                            position = Some(Position {
                                entry_price: price, entry_time: candle.time,
                                direction: "SHORT".to_string(),
                                stop_loss: Some(price + sl_pips * ic.pip_value),
                                take_profit: Some(price - tp_pips * ic.pip_value),
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}

/// Liquidity Sweep - Fake breakout at swing levels + RSI exhaustion
fn simulate_liquidity_sweep(ic: &InstrumentConfig,
    symbol: &str, sl_pips: f64, tp_pips: f64, candles: &[Candle], initial_capital: f64,
) -> (Vec<TradeResponse>, Vec<EquityPoint>) {
    let mut trades = Vec::new();
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut position: Option<Position> = None;

    let rsi_values = rsi(candles, 14);
    let n = candles.len();
    let swing_lookback = 20;
    let sweep_window = 3;
    let warmup = swing_lookback + sweep_window + 10;

    for i in warmup..n {
        let candle = &candles[i];
        let price = candle.close;

        let unrealized = position.as_ref().map(|p| match p.direction.as_str() {
            "LONG" => (price - p.entry_price) * ic.position_size * ic.lot_multiplier,
            "SHORT" => (p.entry_price - price) * ic.position_size * ic.lot_multiplier,
            _ => 0.0,
        }).unwrap_or(0.0);
        equity_curve.push(EquityPoint { time: candle.time, equity: equity + unrealized });

        if let Some(ref pos) = position.clone() {
            if let Some((exit_price, reason)) = check_sl_tp(pos, candle) {
                let (pnl, trade) = create_trade(&ic, symbol, pos, exit_price, candle.time, reason);
                equity += pnl; trades.push(trade); position = None;
            }
        }

        if position.is_none() {
            // Find swing high/low from before the sweep window
            let lookback_end = i.saturating_sub(sweep_window);
            let lookback_start = lookback_end.saturating_sub(swing_lookback);
            if lookback_start >= lookback_end { continue; }

            let swing_high = candles[lookback_start..lookback_end].iter()
                .map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
            let swing_low = candles[lookback_start..lookback_end].iter()
                .map(|c| c.low).fold(f64::INFINITY, f64::min);

            // Check for sweep in recent candles
            let recent = &candles[lookback_end..=i];
            let broke_high = recent.iter().any(|c| c.high > swing_high);
            let broke_low = recent.iter().any(|c| c.low < swing_low);
            let closed_below = price < swing_high;
            let closed_above = price > swing_low;

            // RSI exhaustion during sweep
            let rsi_slice = &rsi_values[lookback_end.min(rsi_values.len().saturating_sub(1))..i.min(rsi_values.len())];
            let max_rsi = rsi_slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let min_rsi = rsi_slice.iter().cloned().fold(f64::INFINITY, f64::min);

            // Bear trap (long): broke below, closed back above, RSI was oversold
            if broke_low && closed_above && min_rsi <= 30.0 {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "LONG".to_string(),
                    stop_loss: Some(price - sl_pips * ic.pip_value),
                    take_profit: Some(price + tp_pips * ic.pip_value),
                });
            }
            // Bull trap (short): broke above, closed back below, RSI was overbought
            else if broke_high && closed_below && max_rsi >= 70.0 {
                position = Some(Position {
                    entry_price: price, entry_time: candle.time,
                    direction: "SHORT".to_string(),
                    stop_loss: Some(price + sl_pips * ic.pip_value),
                    take_profit: Some(price - tp_pips * ic.pip_value),
                });
            }
        }
    }

    if let Some(pos) = position {
        if let Some(last) = candles.last() {
            let (pnl, trade) = create_trade(&ic, symbol, &pos, last.close, last.time, "End of Test");
            equity += pnl; trades.push(trade);
        }
    }
    (trades, equity_curve)
}
