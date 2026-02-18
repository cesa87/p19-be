//! Signal generation for trading strategies
//! 
//! Generates BUY/SELL signals based on strategy type and current market data

use crate::models::Candle;
use crate::engine::indicators::{sma, ema, rsi, macd, stochastic, adx, donchian, bollinger_bands, atr};
use crate::mrate::{MrateOutput, Regime, FavoredInstrument, TrendDirection};
use chrono::{DateTime, Utc, Timelike};

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
}

/// A single condition in a strategy's signal breakdown
#[derive(Debug, Clone, serde::Serialize)]
pub struct SignalCondition {
    pub label: String,
    pub met: bool,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SignalCondition {
    pub fn new(label: &str, met: bool, value: &str) -> Self {
        Self {
            label: label.to_string(),
            met,
            value: value.to_string(),
            target: None,
            description: None,
        }
    }
    
    pub fn with_target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }
    
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
}

#[derive(Debug, Clone)]
pub struct SignalResult {
    pub signal: Signal,
    pub confidence: f64,  // 0.0 to 1.0
    pub reason: String,
    pub conditions: Vec<SignalCondition>,
    pub direction: Option<String>,  // "LONG" or "SHORT" - what the strategy is looking for
}

/// MRATE Threshold Relaxation
/// 
/// When MRATE strongly favors a strategy category (high weight), we can
/// relax entry thresholds slightly to capture more opportunities.
/// 
/// Returns a multiplier to apply to thresholds:
/// - For "needs to be above X" thresholds: multiply by relaxation (lowers threshold)
/// - For "needs to be below X" thresholds: divide by relaxation (raises threshold)
pub fn calculate_mrate_relaxation(mrate_weight: Option<f64>) -> f64 {
    match mrate_weight {
        Some(w) if w >= 0.95 => 0.85,  // 15% relaxation at 95%+ weight
        Some(w) if w >= 0.90 => 0.88,  // 12% relaxation at 90%+ weight
        Some(w) if w >= 0.80 => 0.92,  // 8% relaxation at 80%+ weight
        Some(w) if w >= 0.70 => 0.95,  // 5% relaxation at 70%+ weight
        _ => 1.0,  // No relaxation below 70% or if no MRATE
    }
}

impl SignalResult {
    pub fn hold(reason: &str) -> Self {
        Self {
            signal: Signal::Hold,
            confidence: 0.0,
            reason: reason.to_string(),
            conditions: Vec::new(),
            direction: None,
        }
    }
    
    pub fn hold_with_conditions(reason: &str, conditions: Vec<SignalCondition>, direction: &str) -> Self {
        Self {
            signal: Signal::Hold,
            confidence: 0.0,
            reason: reason.to_string(),
            conditions,
            direction: Some(direction.to_string()),
        }
    }
    
    pub fn buy(confidence: f64, reason: &str) -> Self {
        Self {
            signal: Signal::Buy,
            confidence,
            reason: reason.to_string(),
            conditions: Vec::new(),
            direction: Some("LONG".to_string()),
        }
    }
    
    pub fn buy_with_conditions(confidence: f64, reason: &str, conditions: Vec<SignalCondition>) -> Self {
        Self {
            signal: Signal::Buy,
            confidence,
            reason: reason.to_string(),
            conditions,
            direction: Some("LONG".to_string()),
        }
    }
    
    pub fn sell(confidence: f64, reason: &str) -> Self {
        Self {
            signal: Signal::Sell,
            confidence,
            reason: reason.to_string(),
            conditions: Vec::new(),
            direction: Some("SHORT".to_string()),
        }
    }
    
    pub fn sell_with_conditions(confidence: f64, reason: &str, conditions: Vec<SignalCondition>) -> Self {
        Self {
            signal: Signal::Sell,
            confidence,
            reason: reason.to_string(),
            conditions,
            direction: Some("SHORT".to_string()),
        }
    }
}

/// Generate signal based on strategy type
/// 
/// `mrate_weight` is the MRATE strategy category weight (0.0-1.0). When high,
/// this enables threshold relaxation to capture more opportunities in favorable regimes.
/// `mrate_output` is the full MRATE data for regime-driven strategies.
pub fn generate_signal(
    strategy_type: &str,
    candles: &[Candle],
    params: &serde_json::Value,
    mrate_weight: Option<f64>,
    mrate_output: Option<&MrateOutput>,
) -> SignalResult {
    if candles.len() < 50 {
        let n = candles.len();
        let progress = (n as f64 / 50.0 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/50 candles", n))
                .with_target("50 candles minimum")
                .with_description(&format!("{:.0}% complete — collecting price history", progress)),
            SignalCondition::new("Strategy Analysis", false, "Pending")
                .with_target(&format!("Strategy: {}", strategy_type))
                .with_description("Will begin analysis once sufficient data is available"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Collecting data: {}/50 candles ({:.0}%)", n, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    match strategy_type {
        "sma_crossover" => sma_crossover_signal(candles, mrate_weight),
        "macd_crossover" => macd_crossover_signal(candles, mrate_weight),
        "macd_divergence" => macd_divergence_signal(candles, mrate_weight),
        "donchian_breakout" => donchian_breakout_signal(candles, mrate_weight),
        "adx_trend" => adx_trend_signal(candles, params, mrate_weight),
        "rsi_mean_reversion" => rsi_mean_reversion_signal(candles, params, mrate_weight),
        "rsi_reversion_2" => rsi_reversion_2_signal(candles, params, mrate_weight, mrate_output),
        "bollinger_bounce" | "bollinger_mean_reversion" => bollinger_mean_reversion_signal(candles, mrate_weight),
        "stochastic_crossover" => stochastic_crossover_signal(candles, params, mrate_weight),
        "ema_ribbon" => ema_ribbon_signal(candles, mrate_weight),
        "triple_screen" => triple_screen_signal(candles, mrate_weight),
        "london_breakout" => london_breakout_signal(candles, mrate_weight),
        "quant_gold_momentum" => quant_gold_momentum_signal(candles, params, mrate_weight),
        "london_ny_trend_continuation" => london_ny_trend_continuation_signal(candles, params, mrate_weight),
        "london_ny_trend_2" => london_ny_trend_2_signal(candles, params, mrate_weight),
        "forecast_confidence" => forecast_confidence_signal(candles, params, mrate_weight),
        "volatility_expansion" => volatility_expansion_signal(candles, params, mrate_weight),
        "trendline_bounce" => trendline_bounce_signal(candles, params, mrate_weight),
        "liquidity_sweep" => liquidity_sweep_signal(candles, params, mrate_weight),
        "macro_aligned_momentum" => macro_aligned_momentum_signal(candles, params, mrate_weight),
        "mrate_regime_trader" => mrate_regime_trader_signal(candles, params, mrate_output),
        "real_yield_momentum" => real_yield_momentum_signal(candles, params, mrate_output),
        "gold_dxy_divergence" => gold_dxy_divergence_signal(candles, params, mrate_output),
        "fomc_volatility" => fomc_volatility_signal(candles, params, mrate_output),
        _ => custom_momentum_signal(candles, mrate_weight),  // Fallback for custom strategies
    }
}

fn sma_crossover_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let fast = sma(candles, 9);
    let slow = sma(candles, 21);
    
    let i = candles.len() - 1;
    if i < 1 || i >= fast.len() || i >= slow.len() {
        let conditions = vec![
            SignalCondition::new("SMA(9)", false, "Calculating...")
                .with_target("Fast moving average"),
            SignalCondition::new("SMA(21)", false, "Calculating...")
                .with_target("Slow moving average"),
            SignalCondition::new("Crossover", false, "Pending")
                .with_target("SMA9 crosses SMA21"),
        ];
        return SignalResult::hold_with_conditions("Calculating indicators...", conditions, "NEUTRAL");
    }
    
    let price = candles[i].close;
    let fast_curr = fast[i];
    let slow_curr = slow[i];
    let fast_prev = fast[i - 1];
    let slow_prev = slow[i - 1];
    
    // Determine trend direction
    let is_uptrend = fast_curr > slow_curr;
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    
    // Check for crossovers
    let golden_cross = fast_prev <= slow_prev && fast_curr > slow_curr;
    let death_cross = fast_prev >= slow_prev && fast_curr < slow_curr;
    
    // Calculate how close we are to a crossover
    let sma_diff = fast_curr - slow_curr;
    let sma_diff_prev = fast_prev - slow_prev;
    let converging = sma_diff.abs() < sma_diff_prev.abs();
    
    let conditions = vec![
        SignalCondition::new("SMA(9) - Fast", true, &format!("${:.2}", fast_curr))
            .with_description(&format!("9-period simple moving average")),
        SignalCondition::new("SMA(21) - Slow", true, &format!("${:.2}", slow_curr))
            .with_description(&format!("21-period simple moving average")),
        SignalCondition::new("Trend Direction", true, if is_uptrend { "BULLISH ↑" } else { "BEARISH ↓" })
            .with_target(if is_uptrend { "SMA9 > SMA21" } else { "SMA9 < SMA21" })
            .with_description(&format!("Diff: {:.2} pips", sma_diff.abs())),
        SignalCondition::new("Golden Cross (BUY)", golden_cross, 
            if golden_cross { "✓ TRIGGERED" } else if is_uptrend { "Already bullish" } else if converging { "Converging..." } else { "Waiting..." })
            .with_target("SMA9 crosses ABOVE SMA21")
            .with_description("Fast MA crosses above slow MA = bullish signal"),
        SignalCondition::new("Death Cross (SELL)", death_cross,
            if death_cross { "✓ TRIGGERED" } else if !is_uptrend { "Already bearish" } else if converging { "Converging..." } else { "Waiting..." })
            .with_target("SMA9 crosses BELOW SMA21")
            .with_description("Fast MA crosses below slow MA = bearish signal"),
    ];
    
    // Golden cross
    if golden_cross {
        return SignalResult::buy_with_conditions(0.7, "SMA(9) crossed above SMA(21) - Golden Cross", conditions);
    }
    
    // Death cross
    if death_cross {
        return SignalResult::sell_with_conditions(0.7, "SMA(9) crossed below SMA(21) - Death Cross", conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("SMA9: ${:.2}, SMA21: ${:.2} | {}", fast_curr, slow_curr, if converging { "Converging" } else { "Diverging" }),
        conditions,
        direction
    )
}

fn macd_crossover_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let (macd_line, signal_line, histogram) = macd(candles, 12, 26, 9);
    
    let i = candles.len() - 1;
    if i < 1 || i >= macd_line.len() {
        let conditions = vec![
            SignalCondition::new("MACD Line", false, "Calculating...")
                .with_target("12-26 EMA difference"),
            SignalCondition::new("Signal Line", false, "Calculating...")
                .with_target("9-period EMA of MACD"),
            SignalCondition::new("Histogram", false, "Pending")
                .with_target("MACD - Signal"),
            SignalCondition::new("Crossover", false, "Pending")
                .with_target("MACD crosses Signal"),
        ];
        return SignalResult::hold_with_conditions("Calculating MACD...", conditions, "NEUTRAL");
    }
    
    let macd_curr = macd_line[i];
    let signal_curr = signal_line[i];
    let macd_prev = macd_line[i - 1];
    let signal_prev = signal_line[i - 1];
    let hist = histogram[i];
    let hist_prev = histogram[i - 1];
    
    // Determine direction based on MACD position
    let is_bullish = macd_curr > signal_curr;
    let direction = if is_bullish { "LONG" } else { "SHORT" };
    
    // Check for crossovers
    let bullish_cross = macd_prev <= signal_prev && macd_curr > signal_curr;
    let bearish_cross = macd_prev >= signal_prev && macd_curr < signal_curr;
    
    // Check momentum
    let hist_growing = hist.abs() > hist_prev.abs();
    let hist_positive = hist > 0.0;
    let converging = !hist_growing && ((is_bullish && hist < hist_prev) || (!is_bullish && hist > hist_prev));
    
    let conditions = vec![
        SignalCondition::new("MACD Line", true, &format!("{:.4}", macd_curr))
            .with_description("Fast EMA(12) - Slow EMA(26)"),
        SignalCondition::new("Signal Line", true, &format!("{:.4}", signal_curr))
            .with_description("9-period EMA of MACD line"),
        SignalCondition::new("Histogram", true, &format!("{:.4}", hist))
            .with_target(if hist_positive { "> 0 (Bullish momentum)" } else { "< 0 (Bearish momentum)" })
            .with_description(if hist_growing { "Momentum increasing" } else { "Momentum decreasing" }),
        SignalCondition::new("Trend Direction", true, if is_bullish { "BULLISH ↑" } else { "BEARISH ↓" })
            .with_target(if is_bullish { "MACD > Signal" } else { "MACD < Signal" }),
        SignalCondition::new("Bullish Crossover (BUY)", bullish_cross,
            if bullish_cross { "✓ TRIGGERED" } else if is_bullish { "Already bullish" } else if converging { "Converging..." } else { "Waiting..." })
            .with_target("MACD crosses ABOVE Signal")
            .with_description("Buy signal when MACD crosses above signal line"),
        SignalCondition::new("Bearish Crossover (SELL)", bearish_cross,
            if bearish_cross { "✓ TRIGGERED" } else if !is_bullish { "Already bearish" } else if converging { "Converging..." } else { "Waiting..." })
            .with_target("MACD crosses BELOW Signal")
            .with_description("Sell signal when MACD crosses below signal line"),
    ];
    
    // MACD crosses above signal
    if bullish_cross {
        let confidence = if hist > 0.0 { 0.8 } else { 0.6 };
        return SignalResult::buy_with_conditions(confidence, "MACD crossed above signal line", conditions);
    }
    
    // MACD crosses below signal
    if bearish_cross {
        let confidence = if hist < 0.0 { 0.8 } else { 0.6 };
        return SignalResult::sell_with_conditions(confidence, "MACD crossed below signal line", conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("MACD: {:.4}, Signal: {:.4} | {}", macd_curr, signal_curr, if converging { "Converging" } else { "Diverging" }),
        conditions,
        direction
    )
}

fn macd_divergence_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let (macd_line, signal_line, histogram) = macd(candles, 12, 26, 9);
    let lookback = 10;
    
    let i = candles.len() - 1;
    if i < lookback || i >= histogram.len() {
        let conditions = vec![
            SignalCondition::new("MACD Histogram", false, "Calculating...")
                .with_target("Needs 10+ candles"),
            SignalCondition::new("Price Pattern", false, "Scanning...")
                .with_description("Looking for price lows/highs"),
            SignalCondition::new("Bullish Divergence", false, "Pending")
                .with_target("Price ↓ + MACD ↑"),
            SignalCondition::new("Bearish Divergence", false, "Pending")
                .with_target("Price ↑ + MACD ↓"),
        ];
        return SignalResult::hold_with_conditions("Calculating divergence...", conditions, "NEUTRAL");
    }
    
    // Find local lows/highs
    let price_lows: Vec<f64> = candles[i-lookback..i].iter().map(|c| c.low).collect();
    let hist_slice: Vec<f64> = histogram[i-lookback..i].to_vec();
    
    let price_min = price_lows.iter().cloned().fold(f64::INFINITY, f64::min);
    let hist_min = hist_slice.iter().cloned().fold(f64::INFINITY, f64::min);
    
    let price_highs: Vec<f64> = candles[i-lookback..i].iter().map(|c| c.high).collect();
    let price_max = price_highs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let hist_max = hist_slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    
    let price = candles[i].close;
    let hist_curr = histogram[i];
    let macd_curr = macd_line[i];
    let signal_curr = signal_line[i];
    
    // Check for divergence conditions
    let price_lower_low = candles[i].low < price_min;
    let macd_higher_low = histogram[i] > hist_min && hist_min < 0.0;
    let price_higher_high = candles[i].high > price_max;
    let macd_lower_high = histogram[i] < hist_max && hist_max > 0.0;
    
    let bullish_divergence = price_lower_low && macd_higher_low;
    let bearish_divergence = price_higher_high && macd_lower_high;
    
    // Determine current bias
    let looking_long = hist_curr < 0.0;  // Below zero, looking for bullish divergence
    
    let conditions = vec![
        SignalCondition {
            label: "MACD Histogram".to_string(),
            met: true,
            value: format!("{:.4}", hist_curr),
            target: None,
            description: None,
        },
        SignalCondition {
            label: "MACD vs Signal".to_string(),
            met: (looking_long && macd_curr > signal_curr) || (!looking_long && macd_curr < signal_curr),
            value: format!("{:.4} vs {:.4}", macd_curr, signal_curr),
            target: None,
            description: None,
        },
        SignalCondition {
            label: "Price New Low".to_string(),
            met: price_lower_low,
            value: format!("{:.2} vs {:.2}", candles[i].low, price_min),
            target: Some("Lower than recent".to_string()),
            description: None,
        },
        SignalCondition {
            label: "MACD Higher Low".to_string(),
            met: macd_higher_low,
            value: format!("{:.4} vs {:.4}", hist_curr, hist_min),
            target: Some("Divergence".to_string()),
            description: None,
        },
        SignalCondition {
            label: "Bullish Divergence".to_string(),
            met: bullish_divergence,
            value: if bullish_divergence { "✓ DETECTED" } else { "Waiting..." }.to_string(),
            target: Some("Price ↓ + MACD ↑".to_string()),
            description: None,
        },
        SignalCondition {
            label: "Bearish Divergence".to_string(),
            met: bearish_divergence,
            value: if bearish_divergence { "✓ DETECTED" } else { "Waiting..." }.to_string(),
            target: Some("Price ↑ + MACD ↓".to_string()),
            description: None,
        },
    ];
    
    // Bullish divergence
    if bullish_divergence {
        return SignalResult::buy_with_conditions(0.75, "Bullish divergence: price lower low, MACD higher low", conditions);
    }
    
    // Bearish divergence
    if bearish_divergence {
        return SignalResult::sell_with_conditions(0.75, "Bearish divergence: price higher high, MACD lower high", conditions);
    }
    
    let direction = if looking_long { "LONG" } else { "SHORT" };
    SignalResult::hold_with_conditions("No divergence detected", conditions, direction)
}

fn donchian_breakout_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let (upper, _, lower) = donchian(candles, 20);
    
    let i = candles.len() - 1;
    if i < 1 || i >= upper.len() {
        let conditions = vec![
            SignalCondition::new("20-Period High", false, "Calculating...")
                .with_target("Upper channel boundary"),
            SignalCondition::new("20-Period Low", false, "Calculating...")
                .with_target("Lower channel boundary"),
            SignalCondition::new("Breakout Signal", false, "Pending")
                .with_target("Price breaks channel"),
        ];
        return SignalResult::hold_with_conditions("Calculating Donchian channels...", conditions, "NEUTRAL");
    }
    
    let prev_upper = upper[i - 1];
    let prev_lower = lower[i - 1];
    let price = candles[i].close;
    let high = candles[i].high;
    let low = candles[i].low;
    
    // Calculate how close we are to breakout
    let dist_to_upper = ((prev_upper - high) / prev_upper * 100.0).max(0.0);
    let dist_to_lower = ((low - prev_lower) / prev_lower * 100.0).max(0.0);
    let breakout_up = high > prev_upper && prev_upper > 0.0;
    let breakout_down = low < prev_lower && prev_lower > 0.0;
    
    // Determine bias based on price position
    let mid = (prev_upper + prev_lower) / 2.0;
    let looking_long = price > mid;
    
    let conditions = vec![
        SignalCondition {
            label: "20-Period High".to_string(),
            met: breakout_up,
            value: format!("{:.2}", prev_upper),
            target: Some(format!("Break > {:.2}", prev_upper)),
            description: None,
        },
        SignalCondition {
            label: "Current High".to_string(),
            met: breakout_up,
            value: format!("{:.2}", high),
            target: None,
            description: None,
        },
        SignalCondition {
            label: "Distance to Breakout".to_string(),
            met: dist_to_upper < 0.1 || dist_to_lower < 0.1,
            value: if looking_long { format!("{:.2}%", dist_to_upper) } else { format!("{:.2}%", dist_to_lower) },
            target: Some("< 0.1%".to_string()),
            description: None,
        },
        SignalCondition {
            label: "20-Period Low".to_string(),
            met: breakout_down,
            value: format!("{:.2}", prev_lower),
            target: Some(format!("Break < {:.2}", prev_lower)),
            description: None,
        },
    ];
    
    // Breakout above upper channel
    if breakout_up {
        return SignalResult::buy_with_conditions(0.7, &format!("Breakout above 20-period high {:.2}", prev_upper), conditions);
    }
    
    // Breakout below lower channel
    if breakout_down {
        return SignalResult::sell_with_conditions(0.7, &format!("Breakout below 20-period low {:.2}", prev_lower), conditions);
    }
    
    let direction = if looking_long { "LONG" } else { "SHORT" };
    SignalResult::hold_with_conditions(
        &format!("Price {:.2} within channel [{:.2}, {:.2}]", price, prev_lower, prev_upper),
        conditions,
        direction,
    )
}

fn adx_trend_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let base_threshold = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(25.0);
    // MRATE relaxation: lower ADX threshold when regime favors this strategy
    let threshold = base_threshold * relaxation;
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    
    let i = candles.len() - 1;
    if i >= adx_values.len() || i < 1 {
        let conditions = vec![
            SignalCondition::new("ADX", false, "Calculating...")
                .with_target(&format!("> {:.0} for strong trend", threshold)),
            SignalCondition::new("+DI (Bullish)", false, "Calculating...")
                .with_description("Positive directional indicator"),
            SignalCondition::new("-DI (Bearish)", false, "Calculating...")
                .with_description("Negative directional indicator"),
            SignalCondition::new("Trend Direction", false, "Pending")
                .with_target("+DI vs -DI determines direction"),
        ];
        return SignalResult::hold_with_conditions("Calculating ADX...", conditions, "NEUTRAL");
    }
    
    let adx_val = adx_values[i];
    let adx_prev = adx_values[i - 1];
    let plus = plus_di[i];
    let minus = minus_di[i];
    
    let is_strong_trend = adx_val >= threshold;
    let is_bullish = plus > minus;
    let direction = if is_bullish { "LONG" } else { "SHORT" };
    let adx_rising = adx_val > adx_prev;
    
    // Build conditions - only show the relevant DI condition for current direction
    let mut conditions = vec![
        SignalCondition::new("ADX Strength", is_strong_trend, &format!("{:.1}", adx_val))
            .with_target(&format!("> {:.0}", threshold))
            .with_description(if is_strong_trend { 
                if adx_rising { "Strong trend, strengthening" } else { "Strong trend, weakening" }
            } else { 
                "Weak/ranging market - no clear trend" 
            }),
    ];
    
    // Show DI conditions relevant to the detected direction
    if is_bullish {
        conditions.push(
            SignalCondition::new("+DI (Bullish Force)", true, &format!("{:.1}", plus))
                .with_target(&format!("> -DI ({:.1})", minus))
                .with_description("Buying pressure exceeds selling pressure")
        );
    } else {
        conditions.push(
            SignalCondition::new("-DI (Bearish Force)", true, &format!("{:.1}", minus))
                .with_target(&format!("> +DI ({:.1})", plus))
                .with_description("Selling pressure exceeds buying pressure")
        );
    }
    
    conditions.push(
        SignalCondition::new("Trend Direction", true, if is_bullish { "BULLISH ↑" } else { "BEARISH ↓" })
            .with_target(if is_bullish { "+DI > -DI" } else { "-DI > +DI" })
            .with_description(&format!("DI spread: {:.1}", (plus - minus).abs()))
    );
    
    conditions.push(
        SignalCondition::new("Entry Signal", is_strong_trend, 
            if is_strong_trend { "✓ Ready to enter" } else { "Waiting for trend strength" })
            .with_target(&format!("ADX > {:.0} + clear direction", threshold))
            .with_description(if is_strong_trend {
                if is_bullish { "Strong bullish trend confirmed" } else { "Strong bearish trend confirmed" }
            } else {
                "No entry - market is choppy/ranging"
            })
    );
    
    if !is_strong_trend {
        return SignalResult::hold_with_conditions(
            &format!("ADX {:.1} < {:.1} - weak trend, no entry", adx_val, threshold),
            conditions,
            direction
        );
    }
    
    // Strong trend confirmed
    if is_bullish {
        return SignalResult::buy_with_conditions(
            0.65, 
            &format!("ADX {:.1} strong trend, +DI({:.1}) > -DI({:.1}) - bullish", adx_val, plus, minus),
            conditions
        );
    } else {
        return SignalResult::sell_with_conditions(
            0.65, 
            &format!("ADX {:.1} strong trend, -DI({:.1}) > +DI({:.1}) - bearish", adx_val, minus, plus),
            conditions
        );
    }
}

fn rsi_mean_reversion_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let base_oversold = params.get("oversold").and_then(|v| v.as_f64()).unwrap_or(30.0);
    let base_overbought = params.get("overbought").and_then(|v| v.as_f64()).unwrap_or(70.0);
    // MRATE relaxation: widen entry zones when regime favors mean reversion
    let oversold = base_oversold + (5.0 * (1.0 - relaxation));  // 30 -> 35 when relaxed
    let overbought = base_overbought - (5.0 * (1.0 - relaxation));  // 70 -> 65 when relaxed
    
    let rsi_values = rsi(candles, 14);
    let i = candles.len() - 1;
    
    if i >= rsi_values.len() {
        let conditions = vec![
            SignalCondition::new("RSI Value", false, "Calculating...")
                .with_target("14-period RSI"),
            SignalCondition::new("Oversold Zone", false, "Pending")
                .with_target(&format!("< {:.0}", oversold)),
            SignalCondition::new("Overbought Zone", false, "Pending")
                .with_target(&format!("< {:.0}", overbought)),
        ];
        return SignalResult::hold_with_conditions("Calculating RSI...", conditions, "NEUTRAL");
    }
    
    let rsi_val = rsi_values[i];
    let rsi_prev = if i > 0 { rsi_values[i - 1] } else { rsi_val };
    let is_oversold = rsi_val < oversold;
    let is_overbought = rsi_val > overbought;
    let is_turning_up = rsi_val > rsi_prev;
    let is_turning_down = rsi_val < rsi_prev;
    
    // Determine bias
    let looking_long = rsi_val < 50.0;
    
    let conditions = vec![
        SignalCondition {
            label: "RSI Value".to_string(),
            met: true,
            value: format!("{:.1}", rsi_val),
            target: None,
            description: None,
        },
        SignalCondition {
            label: format!("RSI < {} (Oversold)", oversold),
            met: is_oversold,
            value: format!("{:.1}", rsi_val),
            target: Some(format!("< {:.0}", oversold)),
            description: None,
        },
        SignalCondition {
            label: format!("RSI > {} (Overbought)", overbought),
            met: is_overbought,
            value: format!("{:.1}", rsi_val),
            target: Some(format!("> {:.0}", overbought)),
            description: None,
        },
        SignalCondition {
            label: "RSI Turning".to_string(),
            met: (looking_long && is_turning_up) || (!looking_long && is_turning_down),
            value: if is_turning_up { "↑ Up" } else if is_turning_down { "↓ Down" } else { "→ Flat" }.to_string(),
            target: Some(if looking_long { "Turning up" } else { "Turning down" }.to_string()),
            description: None,
        },
    ];
    
    if is_oversold {
        return SignalResult::buy_with_conditions(0.7, &format!("RSI {:.1} < {:.1} - oversold", rsi_val, oversold), conditions);
    }
    
    if is_overbought {
        return SignalResult::sell_with_conditions(0.7, &format!("RSI {:.1} > {:.1} - overbought", rsi_val, overbought), conditions);
    }
    
    let direction = if looking_long { "LONG" } else { "SHORT" };
    SignalResult::hold_with_conditions(&format!("RSI {:.1} - neutral zone", rsi_val), conditions, direction)
}

/// RSI Reversion 2.0 - Enhanced RSI Mean Reversion Strategy
/// 
/// Builds on the successful RSI mean reversion with additional filters:
/// 1. TREND FILTER: EMA200 determines bias - only SHORT below, only LONG above
/// 2. HTF TREND BIAS: Optional parameter to override local trend with higher timeframe direction
/// 3. ADX FILTER: Requires trending market (ADX > threshold) to avoid chop
/// 4. RSI MOMENTUM: RSI must be turning (confirms reversal starting)
/// 5. VOLATILITY FILTER: ATR within normal range (avoids news spikes)
/// 6. DYNAMIC SL/TP: ATR-based stop loss and take profit
/// 
/// **IMPORTANT**: Set `htf_trend_bias` to "LONG" or "SHORT" to align with higher timeframe.
/// This prevents counter-trend trades during strong moves.
/// 
/// All parameters configurable for optimization.
fn rsi_reversion_2_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>, mrate_output: Option<&MrateOutput>) -> SignalResult {
    // ═══════════════════════════════════════════════════════════════════
    // MRATE THRESHOLD RELAXATION
    // ═══════════════════════════════════════════════════════════════════
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let mrate_active = relaxation < 1.0;
    
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    
    // RSI parameters
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let rsi_oversold_base = params.get("oversold").and_then(|v| v.as_f64()).unwrap_or(30.0);
    let rsi_overbought_base = params.get("overbought").and_then(|v| v.as_f64()).unwrap_or(70.0);
    
    // Apply MRATE relaxation to RSI thresholds
    let rsi_oversold = rsi_oversold_base / relaxation;  // e.g., 30 / 0.85 = 35.3 at 100% weight
    let rsi_overbought = rsi_overbought_base * relaxation;  // e.g., 70 * 0.85 = 59.5 at 100% weight
    
    // Trend filter
    let trend_ema_period = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let use_trend_filter = params.get("use_trend_filter").and_then(|v| v.as_bool()).unwrap_or(true);
    
    // Higher timeframe trend bias - derived from MRATE regime if available
    // MRATE already analyzes H4 trends, so use that intelligence
    let (htf_long_only, htf_short_only) = if let Some(mrate) = mrate_output {
        // Use MRATE regime to determine HTF bias
        match mrate.regime {
            crate::mrate::Regime::GoldSuperBull | crate::mrate::Regime::Trend => {
                // In bullish regimes, block shorts for gold
                (true, false)
            }
            crate::mrate::Regime::Panic => {
                // Panic = safe haven gold demand = block shorts
                (true, false)
            }
            crate::mrate::Regime::BtcSuperBull => {
                // Risk-on but gold could go either way - no bias
                (false, false)
            }
            crate::mrate::Regime::Choppy => {
                // Choppy = mean reversion OK both ways - no bias
                (false, false)
            }
        }
    } else {
        // Fallback to manual param if no MRATE data
        let htf_trend_bias = params.get("htf_trend_bias")
            .and_then(|v| v.as_str())
            .unwrap_or("NEUTRAL");
        (htf_trend_bias == "LONG", htf_trend_bias == "SHORT")
    };
    
    // ADX filter
    let adx_threshold_base = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let adx_threshold = adx_threshold_base * relaxation;  // Relaxed when MRATE favors
    let use_adx_filter = params.get("use_adx_filter").and_then(|v| v.as_bool()).unwrap_or(true);
    
    // Volatility filter
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let vol_filter_low = params.get("vol_filter_low").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let vol_filter_high = params.get("vol_filter_high").and_then(|v| v.as_f64()).unwrap_or(2.0);
    let use_vol_filter = params.get("use_vol_filter").and_then(|v| v.as_bool()).unwrap_or(true);
    
    // Momentum confirmation
    let use_momentum_confirm = params.get("use_momentum_confirm").and_then(|v| v.as_bool()).unwrap_or(true);
    
    // Risk management
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(1.5);
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(2.5);
    
    let i = candles.len() - 1;
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE INDICATORS
    // ═══════════════════════════════════════════════════════════════════
    let rsi_values = rsi(candles, rsi_period);
    let ema_trend = ema(candles, trend_ema_period);
    let atr_values = atr(candles, atr_period);
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    
    // Calculate ATR moving average for volatility regime
    let atr_ma: f64 = if i >= 50 {
        atr_values[i.saturating_sub(50)..i].iter().sum::<f64>() / 50.0
    } else {
        atr_values[..i].iter().sum::<f64>() / i.max(1) as f64
    };
    
    // Ensure we have enough data
    if i < 2 || i >= rsi_values.len() || i >= ema_trend.len() || i >= atr_values.len() || i >= adx_values.len() {
        let progress = (i as f64 / trend_ema_period as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, trend_ema_period))
                .with_target(&format!("{} candles required", trend_ema_period))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("RSI (14)", false, "Pending")
                .with_target("Momentum indicator"),
            SignalCondition::new("EMA Trend", false, "Pending")
                .with_target(&format!("EMA({})", trend_ema_period)),
            SignalCondition::new("ADX Filter", false, "Pending")
                .with_target("Trend strength"),
            SignalCondition::new("Volatility", false, "Pending")
                .with_target("ATR check"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Calculating indicators ({:.0}% complete)...", progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let price = candles[i].close;
    let ema_t = ema_trend[i];
    let rsi_curr = rsi_values[i];
    let rsi_prev = rsi_values[i - 1];
    let rsi_prev2 = if i >= 2 { rsi_values[i - 2] } else { rsi_prev };
    let atr_val = atr_values[i];
    let adx_val = adx_values[i];
    
    if atr_val == 0.0 || atr_ma == 0.0 {
        let conditions = vec![
            SignalCondition::new("ATR Calculation", false, "Zero/Invalid")
                .with_target("Non-zero ATR required")
                .with_description("Waiting for price movement data"),
        ];
        return SignalResult::hold_with_conditions("ATR is zero - insufficient volatility data", conditions, "NEUTRAL");
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // EVALUATE FILTERS
    // ═══════════════════════════════════════════════════════════════════
    
    // Filter 1: Trend direction
    let is_uptrend = price > ema_t;
    let trend_filter_pass = !use_trend_filter || true;  // Always passes, but affects direction
    
    // Filter 2: ADX - Is the market trending?
    let is_trending = adx_val >= adx_threshold;
    let adx_filter_pass = !use_adx_filter || is_trending;
    
    // Filter 3: Volatility regime
    let vol_ratio = atr_val / atr_ma;
    let is_normal_volatility = vol_ratio >= vol_filter_low && vol_ratio <= vol_filter_high;
    let vol_filter_pass = !use_vol_filter || is_normal_volatility;
    
    // Filter 4: RSI at extreme
    let rsi_oversold_met = rsi_curr < rsi_oversold;
    let rsi_overbought_met = rsi_curr > rsi_overbought;
    
    // Filter 5: RSI momentum turning
    let rsi_turning_up = rsi_curr > rsi_prev && rsi_prev <= rsi_prev2;
    let rsi_turning_down = rsi_curr < rsi_prev && rsi_prev >= rsi_prev2;
    let momentum_pass = !use_momentum_confirm || 
        (rsi_oversold_met && rsi_turning_up) || 
        (rsi_overbought_met && rsi_turning_down);
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE RISK PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let stop_loss = atr_val * atr_sl_mult;
    let take_profit = atr_val * atr_tp_mult;
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    
    let mrate_label = if mrate_active {
        format!(" 🚀 ({:.0}% boost)", (1.0 - relaxation) * 100.0)
    } else {
        "".to_string()
    };
    
    let mut conditions = vec![
        SignalCondition::new(
            "Trend Filter (EMA200)",
            trend_filter_pass,
            if is_uptrend { "BULLISH ↑" } else { "BEARISH ↓" }
        ).with_target(if is_uptrend { "Price > EMA200 = LONG only" } else { "Price < EMA200 = SHORT only" })
         .with_description(&format!("Price: ${:.2}, EMA200: ${:.2}", price, ema_t)),
    ];
    
    if use_adx_filter {
        conditions.push(
            SignalCondition::new(
                &format!("ADX Filter{}", mrate_label),
                adx_filter_pass,
                &format!("{:.1}", adx_val)
            ).with_target(&format!(">= {:.0}", adx_threshold))
             .with_description(if is_trending { "Market is trending" } else { "Choppy market - avoid" })
        );
    }
    
    if use_vol_filter {
        conditions.push(
            SignalCondition::new(
                "Volatility Filter",
                vol_filter_pass,
                &format!("{:.2}x avg", vol_ratio)
            ).with_target(&format!("{:.1}x - {:.1}x", vol_filter_low, vol_filter_high))
             .with_description(if !is_normal_volatility && vol_ratio > vol_filter_high { 
                 "⚠️ Volatility spike - likely news" 
             } else if !is_normal_volatility { 
                 "⚠️ Low volatility - dead market" 
             } else { 
                 "Normal volatility" 
             })
        );
    }
    
    // RSI condition based on trend direction
    if is_uptrend {
        conditions.push(
            SignalCondition::new(
                &format!("RSI Oversold{}", mrate_label),
                rsi_oversold_met,
                &format!("{:.1}", rsi_curr)
            ).with_target(&format!("< {:.0}", rsi_oversold))
             .with_description("Looking for oversold bounce in uptrend")
        );
    } else {
        conditions.push(
            SignalCondition::new(
                &format!("RSI Overbought{}", mrate_label),
                rsi_overbought_met,
                &format!("{:.1}", rsi_curr)
            ).with_target(&format!("> {:.0}", rsi_overbought))
             .with_description("Looking for overbought rejection in downtrend")
        );
    }
    
    if use_momentum_confirm {
        let is_turning = if is_uptrend { rsi_turning_up } else { rsi_turning_down };
        conditions.push(
            SignalCondition::new(
                if is_uptrend { "RSI Turning Up" } else { "RSI Turning Down" },
                is_turning,
                if is_turning { "✓ Confirmed" } else { "Waiting..." }
            ).with_target("Momentum shift detected")
             .with_description(&format!("RSI: {:.1} → {:.1} → {:.1}", rsi_prev2, rsi_prev, rsi_curr))
        );
    }
    
    // Add risk parameters
    conditions.push(
        SignalCondition::new(
            "Risk Management",
            true,
            &format!("SL: ${:.2}, TP: ${:.2}", stop_loss, take_profit)
        ).with_target(&format!("{:.1}x / {:.1}x ATR", atr_sl_mult, atr_tp_mult))
         .with_description(&format!("ATR({}): ${:.2}", atr_period, atr_val))
    );
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STATUS MESSAGE
    // ═══════════════════════════════════════════════════════════════════
    let trend_str = if is_uptrend { "BULL" } else { "BEAR" };
    let adx_str = if use_adx_filter {
        format!("ADX: {:.0} {}", adx_val, if is_trending { "✓" } else { "✗" })
    } else {
        "ADX: off".to_string()
    };
    let vol_str = if use_vol_filter {
        format!("Vol: {:.1}x {}", vol_ratio, if is_normal_volatility { "✓" } else { "✗" })
    } else {
        "Vol: off".to_string()
    };
    
    let status = format!("Trend: {} | {} | {} | RSI: {:.1}", trend_str, adx_str, vol_str, rsi_curr);
    
    // ═══════════════════════════════════════════════════════════════════
    // FILTER REJECTIONS (with conditions for UI)
    // ═══════════════════════════════════════════════════════════════════
    if use_adx_filter && !adx_filter_pass {
        return SignalResult::hold_with_conditions(
            &format!("⏸️ {} | Choppy market (ADX {:.0} < {:.0})", status, adx_val, adx_threshold),
            conditions,
            direction
        );
    }
    
    if use_vol_filter && !vol_filter_pass {
        let reason = if vol_ratio > vol_filter_high {
            format!("⏸️ {} | Volatility spike ({:.1}x) - avoid", status, vol_ratio)
        } else {
            format!("⏸️ {} | Low volatility ({:.1}x) - avoid", status, vol_ratio)
        };
        return SignalResult::hold_with_conditions(&reason, conditions, direction);
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // LONG SIGNAL - Uptrend + Oversold + Turning up
    // ═══════════════════════════════════════════════════════════════════
    // BLOCKED if htf_trend_bias is SHORT (don't long into a bearish HTF trend)
    if htf_short_only && rsi_oversold_met {
        // Higher timeframe is bearish - skip longs entirely
        return SignalResult::hold_with_conditions(
            &format!("⏸️ {} | HTF bias is SHORT - longs blocked", status),
            conditions,
            "SHORT"  // Bias towards shorts
        );
    }
    
    if use_trend_filter && is_uptrend && rsi_oversold_met && (!use_momentum_confirm || rsi_turning_up) {
        let rsi_depth = (rsi_oversold - rsi_curr) / 10.0;  // Deeper = more confident
        let adx_bonus = if use_adx_filter && adx_val > adx_threshold { (adx_val - adx_threshold) / 40.0 } else { 0.0 };
        let confidence = (0.65 + rsi_depth.min(0.15) + adx_bonus.min(0.1)).min(0.85);
        
        let mrate_note = if mrate_active { " 🚀" } else { "" };
        return SignalResult::buy_with_conditions(
            confidence,
            &format!("🟢 LONG{} | {} | SL: ${:.2} TP: ${:.2}", mrate_note, status, stop_loss, take_profit),
            conditions
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // SHORT SIGNAL - Downtrend + Overbought + Turning down
    // ═══════════════════════════════════════════════════════════════════
    // BLOCKED if htf_trend_bias is LONG (don't short into a bullish HTF trend)
    if htf_long_only {
        // Higher timeframe is bullish - skip shorts entirely
        return SignalResult::hold_with_conditions(
            &format!("⏸️ {} | HTF bias is LONG - shorts blocked", status),
            conditions,
            "LONG"  // Bias towards longs
        );
    }
    
    if use_trend_filter && !is_uptrend && rsi_overbought_met && (!use_momentum_confirm || rsi_turning_down) {
        let rsi_depth = (rsi_curr - rsi_overbought) / 10.0;  // Higher = more confident
        let adx_bonus = if use_adx_filter && adx_val > adx_threshold { (adx_val - adx_threshold) / 40.0 } else { 0.0 };
        let confidence = (0.65 + rsi_depth.min(0.15) + adx_bonus.min(0.1)).min(0.85);
        
        let mrate_note = if mrate_active { " 🚀" } else { "" };
        return SignalResult::sell_with_conditions(
            confidence,
            &format!("🔴 SHORT{} | {} | SL: ${:.2} TP: ${:.2}", mrate_note, status, stop_loss, take_profit),
            conditions
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // NO TREND FILTER MODE - Original behavior but with extras
    // ═══════════════════════════════════════════════════════════════════
    if !use_trend_filter {
        if rsi_oversold_met && (!use_momentum_confirm || rsi_turning_up) {
            let confidence = 0.70;
            return SignalResult::buy_with_conditions(
                confidence,
                &format!("🟢 LONG | {} | SL: ${:.2} TP: ${:.2}", status, stop_loss, take_profit),
                conditions
            );
        }
        
        if rsi_overbought_met && (!use_momentum_confirm || rsi_turning_down) {
            let confidence = 0.70;
            return SignalResult::sell_with_conditions(
                confidence,
                &format!("🔴 SHORT | {} | SL: ${:.2} TP: ${:.2}", status, stop_loss, take_profit),
                conditions
            );
        }
    }
    
    // Default: Hold with all conditions displayed
    SignalResult::hold_with_conditions(
        &format!("⏳ {} | Waiting for RSI extreme", status),
        conditions,
        direction
    )
}

fn bollinger_mean_reversion_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let mrate_active = relaxation < 1.0;
    // MRATE relaxation: when active, we'll accept touches closer to bands
    let (upper, middle, lower) = bollinger_bands(candles, 20, 2.0);
    
    let i = candles.len() - 1;
    if i >= upper.len() || i >= lower.len() {
        let conditions = vec![
            SignalCondition::new("Upper Band", false, "Calculating...")
                .with_target("2 std dev above SMA20"),
            SignalCondition::new("Middle Band (SMA20)", false, "Calculating...")
                .with_target("20-period moving average"),
            SignalCondition::new("Lower Band", false, "Calculating...")
                .with_target("2 std dev below SMA20"),
            SignalCondition::new("Band Touch", false, "Pending")
                .with_target("Price touches upper or lower band"),
        ];
        return SignalResult::hold_with_conditions("Calculating Bollinger Bands...", conditions, "NEUTRAL");
    }
    
    let price = candles[i].close;
    let low = candles[i].low;
    let high = candles[i].high;
    
    let touched_lower = low <= lower[i];
    let touched_upper = high >= upper[i];
    let band_width = upper[i] - lower[i];
    let position_in_band = (price - lower[i]) / band_width * 100.0;  // 0% = lower, 100% = upper
    
    // Distance to bands
    let dist_to_lower = ((price - lower[i]) / price * 100.0).abs();
    let dist_to_upper = ((upper[i] - price) / price * 100.0).abs();
    
    // Determine bias based on position
    let looking_long = price < middle[i];
    
    let conditions = vec![
        SignalCondition {
            label: "Upper Band".to_string(),
            met: touched_upper,
            value: format!("{:.2}", upper[i]),
            target: Some(format!("Touch > {:.2}", upper[i])),
            description: None,
        },
        SignalCondition {
            label: "Current Price".to_string(),
            met: true,
            value: format!("{:.2}", price),
            target: None,
            description: None,
        },
        SignalCondition {
            label: "Middle Band (SMA20)".to_string(),
            met: true,
            value: format!("{:.2}", middle[i]),
            target: None,
            description: None,
        },
        SignalCondition {
            label: "Lower Band".to_string(),
            met: touched_lower,
            value: format!("{:.2}", lower[i]),
            target: Some(format!("Touch < {:.2}", lower[i])),
            description: None,
        },
        SignalCondition {
            label: "Band Position".to_string(),
            met: position_in_band < 20.0 || position_in_band > 80.0,
            value: format!("{:.0}%", position_in_band),
            target: Some("< 20% or > 80%".to_string()),
            description: None,
        },
        SignalCondition {
            label: "Distance to Target".to_string(),
            met: dist_to_lower < 0.2 || dist_to_upper < 0.2,
            value: if looking_long { format!("{:.2}% to lower", dist_to_lower) } else { format!("{:.2}% to upper", dist_to_upper) },
            target: Some("< 0.2%".to_string()),
            description: None,
        },
    ];
    
    // Price touches lower band - buy
    if touched_lower {
        return SignalResult::buy_with_conditions(0.65, &format!("Price touched lower band {:.2}", lower[i]), conditions);
    }
    
    // Price touches upper band - sell
    if touched_upper {
        return SignalResult::sell_with_conditions(0.65, &format!("Price touched upper band {:.2}", upper[i]), conditions);
    }
    
    let direction = if looking_long { "LONG" } else { "SHORT" };
    SignalResult::hold_with_conditions(
        &format!("BB: {:.2} / {:.2} / {:.2} | Pos: {:.0}%", lower[i], middle[i], upper[i], position_in_band),
        conditions,
        direction,
    )
}

fn stochastic_crossover_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let base_oversold = params.get("oversold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let base_overbought = params.get("overbought").and_then(|v| v.as_f64()).unwrap_or(80.0);
    // MRATE relaxation: widen entry zones when regime favors mean reversion
    let oversold = base_oversold + (5.0 * (1.0 - relaxation));  // 20 -> 25 when relaxed
    let overbought = base_overbought - (5.0 * (1.0 - relaxation));  // 80 -> 75 when relaxed
    
    let (k_values, d_values) = stochastic(candles, 14, 3);
    let i = candles.len() - 1;
    
    if i < 1 || i >= k_values.len() {
        let conditions = vec![
            SignalCondition::new("%K Line", false, "Calculating...")
                .with_target("Stochastic fast line"),
            SignalCondition::new("%D Line", false, "Calculating...")
                .with_target("Stochastic signal line"),
            SignalCondition::new("Zone", false, "Pending")
                .with_target(&format!("Oversold < {:.0} | Overbought > {:.0}", oversold, overbought)),
            SignalCondition::new("Crossover", false, "Pending")
                .with_target("%K crosses %D in extreme zone"),
        ];
        return SignalResult::hold_with_conditions("Calculating Stochastic...", conditions, "NEUTRAL");
    }
    
    let k_curr = k_values[i];
    let d_curr = d_values[i];
    let k_prev = k_values[i - 1];
    let d_prev = d_values[i - 1];
    
    let is_oversold = k_curr < oversold;
    let is_overbought = k_curr > overbought;
    let bullish_cross = k_prev <= d_prev && k_curr > d_curr;
    let bearish_cross = k_prev >= d_prev && k_curr < d_curr;
    let direction = if k_curr < 50.0 { "LONG" } else { "SHORT" };
    
    // Pre-compute formatted strings to avoid borrow issues
    let k_str = format!("{:.1}", k_curr);
    let d_str = format!("{:.1}", d_curr);
    let oversold_val_str = format!("{:.1} (need < {:.0})", k_curr, oversold);
    let overbought_val_str = format!("{:.1} (need > {:.0})", k_curr, overbought);
    let oversold_target = format!("< {:.0}", oversold);
    let overbought_target = format!("> {:.0}", overbought);
    
    let conditions = vec![
        SignalCondition::new("%K Line", true, &k_str)
            .with_description("Fast stochastic (14-period)"),
        SignalCondition::new("%D Line", true, &d_str)
            .with_description("Slow stochastic (3-period SMA of %K)"),
        SignalCondition::new("Oversold Zone", is_oversold, 
            if is_oversold { "✓ In zone" } else { &oversold_val_str })
            .with_target(&oversold_target)
            .with_description("Buy opportunity zone"),
        SignalCondition::new("Overbought Zone", is_overbought,
            if is_overbought { "✓ In zone" } else { &overbought_val_str })
            .with_target(&overbought_target)
            .with_description("Sell opportunity zone"),
        SignalCondition::new("Bullish Cross (BUY)", bullish_cross && is_oversold,
            if bullish_cross && is_oversold { "✓ TRIGGERED" } 
            else if bullish_cross { "Cross occurred, not in zone" }
            else if is_oversold { "In zone, waiting for cross" }
            else { "Waiting..." })
            .with_target("%K crosses above %D in oversold")
            .with_description("Buy when %K crosses above %D below oversold level"),
        SignalCondition::new("Bearish Cross (SELL)", bearish_cross && is_overbought,
            if bearish_cross && is_overbought { "✓ TRIGGERED" }
            else if bearish_cross { "Cross occurred, not in zone" }
            else if is_overbought { "In zone, waiting for cross" }
            else { "Waiting..." })
            .with_target("%K crosses below %D in overbought")
            .with_description("Sell when %K crosses below %D above overbought level"),
    ];
    
    // %K crosses above %D in oversold
    if bullish_cross && is_oversold {
        return SignalResult::buy_with_conditions(0.7, 
            &format!("Stoch %K crossed above %D in oversold ({:.1})", k_curr), conditions);
    }
    
    // %K crosses below %D in overbought
    if bearish_cross && is_overbought {
        return SignalResult::sell_with_conditions(0.7, 
            &format!("Stoch %K crossed below %D in overbought ({:.1})", k_curr), conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("Stoch %K: {:.1}, %D: {:.1} | {}", k_curr, d_curr,
            if is_oversold { "Oversold" } else if is_overbought { "Overbought" } else { "Neutral" }),
        conditions,
        direction
    )
}

fn ema_ribbon_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let ema8 = ema(candles, 8);
    let ema13 = ema(candles, 13);
    let ema21 = ema(candles, 21);
    let ema34 = ema(candles, 34);
    let ema55 = ema(candles, 55);
    
    let i = candles.len() - 1;
    if i >= ema55.len() {
        let conditions = vec![
            SignalCondition::new("EMA 8", false, "Calculating..."),
            SignalCondition::new("EMA 13", false, "Calculating..."),
            SignalCondition::new("EMA 21", false, "Calculating..."),
            SignalCondition::new("EMA 34", false, "Calculating..."),
            SignalCondition::new("EMA 55", false, "Calculating..."),
            SignalCondition::new("Ribbon Alignment", false, "Pending")
                .with_target("All EMAs in order"),
        ];
        return SignalResult::hold_with_conditions("Calculating EMA ribbon...", conditions, "NEUTRAL");
    }
    
    let e8 = ema8[i];
    let e13 = ema13[i];
    let e21 = ema21[i];
    let e34 = ema34[i];
    let e55 = ema55[i];
    
    // Check alignment
    let bullish_aligned = e8 > e13 && e13 > e21 && e21 > e34 && e34 > e55;
    let bearish_aligned = e8 < e13 && e13 < e21 && e21 < e34 && e34 < e55;
    
    // Count how many are in order
    let mut bullish_count = 0;
    if e8 > e13 { bullish_count += 1; }
    if e13 > e21 { bullish_count += 1; }
    if e21 > e34 { bullish_count += 1; }
    if e34 > e55 { bullish_count += 1; }
    
    let direction = if bullish_count >= 2 { "LONG" } else { "SHORT" };
    let is_looking_long = bullish_count >= 2;
    
    // Pre-compute formatted strings
    let e8_str = format!("${:.2}", e8);
    let e13_str = format!("${:.2}", e13);
    let e21_str = format!("${:.2}", e21);
    let e34_str = format!("${:.2}", e34);
    let e55_str = format!("${:.2}", e55);
    
    // Build conditions based on the direction we're looking for
    let mut conditions = vec![
        SignalCondition::new("EMA 8 (Fastest)", true, &e8_str),
        SignalCondition::new("EMA 13", true, &e13_str)
            .with_description(if e8 > e13 { "EMA8 > EMA13 (bullish)" } else { "EMA8 < EMA13 (bearish)" }),
        SignalCondition::new("EMA 21", true, &e21_str)
            .with_description(if e13 > e21 { "EMA13 > EMA21 (bullish)" } else { "EMA13 < EMA21 (bearish)" }),
        SignalCondition::new("EMA 34", true, &e34_str)
            .with_description(if e21 > e34 { "EMA21 > EMA34 (bullish)" } else { "EMA21 < EMA34 (bearish)" }),
        SignalCondition::new("EMA 55 (Slowest)", true, &e55_str)
            .with_description(if e34 > e55 { "EMA34 > EMA55 (bullish)" } else { "EMA34 < EMA55 (bearish)" }),
    ];
    
    // Only show the relevant alignment condition for the current direction
    if is_looking_long {
        let bullish_status = if bullish_aligned { 
            "✓ All EMAs aligned bullish".to_string() 
        } else { 
            format!("{}/4 pairs aligned", bullish_count) 
        };
        conditions.push(
            SignalCondition::new("Full Bullish Alignment", bullish_aligned, &bullish_status)
                .with_target("EMA8 > EMA13 > EMA21 > EMA34 > EMA55")
                .with_description("Strong uptrend when all EMAs stack bullish")
        );
    } else {
        let bearish_status = if bearish_aligned { 
            "✓ All EMAs aligned bearish".to_string() 
        } else { 
            format!("{}/4 pairs aligned", 4 - bullish_count) 
        };
        conditions.push(
            SignalCondition::new("Full Bearish Alignment", bearish_aligned, &bearish_status)
                .with_target("EMA8 < EMA13 < EMA21 < EMA34 < EMA55")
                .with_description("Strong downtrend when all EMAs stack bearish")
        );
    }
    
    // All aligned bullish
    if bullish_aligned {
        return SignalResult::buy_with_conditions(0.75, "EMA ribbon fully aligned bullish", conditions);
    }
    
    // All aligned bearish
    if bearish_aligned {
        return SignalResult::sell_with_conditions(0.75, "EMA ribbon fully aligned bearish", conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("EMA ribbon {}/4 bullish - not fully aligned", bullish_count),
        conditions,
        direction
    )
}

fn triple_screen_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let ema13 = ema(candles, 13);
    let (k_values, d_values) = stochastic(candles, 5, 3);
    
    let i = candles.len() - 1;
    if i < 1 || i >= ema13.len() || i >= k_values.len() {
        let conditions = vec![
            SignalCondition::new("Screen 1: Trend (EMA13)", false, "Calculating...")
                .with_description("Weekly/Daily trend direction"),
            SignalCondition::new("Screen 2: Oscillator (Stoch)", false, "Calculating...")
                .with_description("Entry timing signal"),
            SignalCondition::new("Screen 3: Entry Trigger", false, "Pending")
                .with_description("Precise entry point"),
        ];
        return SignalResult::hold_with_conditions("Calculating Triple Screen...", conditions, "NEUTRAL");
    }
    
    let ema_curr = ema13[i];
    let ema_prev = ema13[i - 1];
    let k_curr = k_values[i];
    let d_curr = d_values[i];
    let k_prev = k_values[i - 1];
    let d_prev = d_values[i - 1];
    
    let trend_up = ema_curr > ema_prev;
    let trend_down = ema_curr < ema_prev;
    let stoch_oversold = k_curr < 30.0;
    let stoch_overbought = k_curr > 70.0;
    let bullish_cross = k_prev <= d_prev && k_curr > d_curr;
    let bearish_cross = k_prev >= d_prev && k_curr < d_curr;
    
    let buy_signal = trend_up && bullish_cross && stoch_oversold;
    let sell_signal = trend_down && bearish_cross && stoch_overbought;
    let direction = if trend_up { "LONG" } else { "SHORT" };
    
    let conditions = vec![
        SignalCondition::new("Screen 1: Trend (EMA13)", true, 
            if trend_up { "UPTREND ↑" } else { "DOWNTREND ↓" })
            .with_target("Determines trade direction")
            .with_description(&format!("EMA: ${:.2} vs prev ${:.2}", ema_curr, ema_prev)),
        SignalCondition::new("Screen 2: Stochastic Zone", 
            (trend_up && stoch_oversold) || (trend_down && stoch_overbought),
            &format!("%K: {:.1}", k_curr))
            .with_target(if trend_up { "< 30 (oversold)" } else { "> 70 (overbought)" })
            .with_description(if trend_up { 
                if stoch_oversold { "In buy zone ✓" } else { "Waiting for oversold" }
            } else {
                if stoch_overbought { "In sell zone ✓" } else { "Waiting for overbought" }
            }),
        SignalCondition::new("Screen 3: Stoch Crossover",
            (trend_up && bullish_cross) || (trend_down && bearish_cross),
            if bullish_cross { "%K crossed above %D" } else if bearish_cross { "%K crossed below %D" } else { "No cross" })
            .with_target(if trend_up { "%K > %D (bullish)" } else { "%K < %D (bearish)" })
            .with_description("Entry trigger confirmation"),
        SignalCondition::new("BUY Signal", buy_signal,
            if buy_signal { "✓ ALL CONDITIONS MET" } else { "Waiting..." })
            .with_target("Uptrend + Oversold + Bullish Cross")
            .with_description("Triple screen buy alignment"),
        SignalCondition::new("SELL Signal", sell_signal,
            if sell_signal { "✓ ALL CONDITIONS MET" } else { "Waiting..." })
            .with_target("Downtrend + Overbought + Bearish Cross")
            .with_description("Triple screen sell alignment"),
    ];
    
    // Buy signal: Uptrend + stoch bullish cross in oversold
    if buy_signal {
        return SignalResult::buy_with_conditions(0.8, "Triple Screen: uptrend + stoch bullish cross in oversold", conditions);
    }
    
    // Sell signal: Downtrend + stoch bearish cross in overbought
    if sell_signal {
        return SignalResult::sell_with_conditions(0.8, "Triple Screen: downtrend + stoch bearish cross in overbought", conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("Trend: {} | Stoch: {:.1} | Waiting for alignment", 
            if trend_up { "UP" } else { "DOWN" }, k_curr),
        conditions,
        direction
    )
}

fn london_breakout_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let range_period = 12;  // ~3 hours of M15 candles
    let i = candles.len() - 1;
    
    if i < range_period {
        let conditions = vec![
            SignalCondition::new("Asian Range High", false, "Calculating...")
                .with_description("Resistance level from Asian session"),
            SignalCondition::new("Asian Range Low", false, "Calculating...")
                .with_description("Support level from Asian session"),
            SignalCondition::new("Breakout Direction", false, "Pending")
                .with_target("Price breaks above high or below low"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Building range: {}/{} candles", i, range_period), 
            conditions, 
            "NEUTRAL"
        );
    }
    
    let range_high = candles[i-range_period..i].iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let range_low = candles[i-range_period..i].iter()
        .map(|c| c.low)
        .fold(f64::INFINITY, f64::min);
    
    let price = candles[i].close;
    let high = candles[i].high;
    let low = candles[i].low;
    
    let breakout_up = price > range_high;
    let breakout_down = price < range_low;
    let range_size = range_high - range_low;
    let mid = (range_high + range_low) / 2.0;
    let direction = if price > mid { "LONG" } else { "SHORT" };
    
    // Calculate proximity to breakout
    let dist_to_high = ((range_high - high) / range_high * 100.0).max(0.0);
    let dist_to_low = ((low - range_low) / range_low * 100.0).max(0.0);
    
    // Pre-compute formatted strings
    let range_high_str = format!("${:.2}", range_high);
    let range_low_str = format!("${:.2}", range_low);
    let range_size_str = format!("${:.2}", range_size);
    let price_str = format!("${:.2}", price);
    let consolidation_desc = format!("{} period consolidation", range_period);
    let position_desc = format!("Position: {:.0}% of range", (price - range_low) / range_size * 100.0);
    let dist_high_str = format!("{:.2}% to breakout", dist_to_high);
    let dist_low_str = format!("{:.2}% to breakout", dist_to_low);
    let bullish_target = format!("Price > ${:.2}", range_high);
    let bearish_target = format!("Price < ${:.2}", range_low);
    
    let conditions = vec![
        SignalCondition::new("Asian Range High", true, &range_high_str)
            .with_description("Resistance - breakout above triggers BUY"),
        SignalCondition::new("Asian Range Low", true, &range_low_str)
            .with_description("Support - breakout below triggers SELL"),
        SignalCondition::new("Range Size", true, &range_size_str)
            .with_description(&consolidation_desc),
        SignalCondition::new("Current Price", true, &price_str)
            .with_description(&position_desc),
        SignalCondition::new("Bullish Breakout (BUY)", breakout_up,
            if breakout_up { "✓ TRIGGERED" } else { &dist_high_str })
            .with_target(&bullish_target)
            .with_description("London session breakout above Asian high"),
        SignalCondition::new("Bearish Breakout (SELL)", breakout_down,
            if breakout_down { "✓ TRIGGERED" } else { &dist_low_str })
            .with_target(&bearish_target)
            .with_description("London session breakout below Asian low"),
    ];
    
    if breakout_up {
        return SignalResult::buy_with_conditions(0.7, 
            &format!("Breakout above range high ${:.2}", range_high), conditions);
    }
    
    if breakout_down {
        return SignalResult::sell_with_conditions(0.7, 
            &format!("Breakout below range low ${:.2}", range_low), conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("Price ${:.2} within range [${:.2}, ${:.2}]", price, range_low, range_high),
        conditions,
        direction
    )
}

/// Quantitative Gold Momentum Reversion Strategy v2
/// 
/// A statistically-driven strategy designed specifically for XAUUSD with regime filters:
/// 
/// ENTRY FILTERS:
/// 1. TREND FILTER: EMA(trend_ema) determines directional bias
/// 2. REGIME FILTER: ADX > threshold confirms trending market (avoids chop)
/// 3. VOLATILITY FILTER: ATR must be within normal range (avoids news spikes)
/// 4. MEAN REVERSION: RSI pullback entry at configurable levels
/// 5. VALUE ZONE: Price within N×ATR of value EMA (not overextended)
/// 6. MOMENTUM: RSI must be turning (confirms reversal)
/// 
/// All parameters are configurable for walk-forward optimization.
fn quant_gold_momentum_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    use crate::engine::indicators::atr;
    
    // ═══════════════════════════════════════════════════════════════════
    // MRATE THRESHOLD RELAXATION
    // ═══════════════════════════════════════════════════════════════════
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let mrate_active = relaxation < 1.0;
    
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS - All can be optimized via walk-forward
    // ═══════════════════════════════════════════════════════════════════
    
    // Trend parameters
    let trend_ema_period = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let value_ema_period = params.get("value_ema").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    
    // RSI parameters (test ranges: oversold 25-40, overbought 60-75)
    // With MRATE relaxation applied
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let rsi_oversold_base = params.get("rsi_oversold").and_then(|v| v.as_f64()).unwrap_or(35.0);
    let rsi_overbought_base = params.get("rsi_overbought").and_then(|v| v.as_f64()).unwrap_or(65.0);
    
    // Apply MRATE relaxation to RSI thresholds:
    // - Oversold: raise the threshold (easier to trigger long) by dividing
    // - Overbought: lower the threshold (easier to trigger short) by multiplying
    let rsi_oversold = rsi_oversold_base / relaxation;  // e.g., 35 / 0.85 = 41.2 at 100% weight
    let rsi_overbought = rsi_overbought_base * relaxation;  // e.g., 65 * 0.85 = 55.3 at 100% weight
    
    // ATR/Volatility parameters (test ranges: 1.0-2.5)
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_distance_mult = params.get("atr_distance").and_then(|v| v.as_f64()).unwrap_or(1.5);
    
    // REGIME FILTERS - Critical for avoiding chop (also relaxed by MRATE)
    let adx_threshold_base = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let adx_threshold = adx_threshold_base * relaxation;  // e.g., 20 * 0.85 = 17 at 100% weight
    let vol_filter_mult = params.get("vol_filter").and_then(|v| v.as_f64()).unwrap_or(2.0); // ATR < 2x average
    
    let i = candles.len() - 1;
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE INDICATORS
    // ═══════════════════════════════════════════════════════════════════
    let ema_trend = ema(candles, trend_ema_period);
    let ema_value = ema(candles, value_ema_period);
    let rsi_values = rsi(candles, rsi_period);
    let atr_values = atr(candles, atr_period);
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    
    // Calculate ATR moving average for volatility regime
    let atr_ma: f64 = if i >= 50 {
        atr_values[i.saturating_sub(50)..i].iter().sum::<f64>() / 50.0
    } else {
        atr_values[..i].iter().sum::<f64>() / i.max(1) as f64
    };
    
    // Ensure we have enough data
    if i < 2 || i >= ema_trend.len() || i >= ema_value.len() || i >= rsi_values.len() || i >= atr_values.len() || i >= adx_values.len() {
        let progress = (i as f64 / trend_ema_period as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, trend_ema_period))
                .with_target(&format!("{} candles required", trend_ema_period))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("RSI Momentum", false, "Pending")
                .with_target("RSI pullback detection"),
            SignalCondition::new("ADX Trend", false, "Pending")
                .with_target("Trending market filter"),
            SignalCondition::new("Value Zone", false, "Pending")
                .with_target("Within ATR distance"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Calculating indicators ({:.0}% complete)...", progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let price = candles[i].close;
    let ema_t = ema_trend[i];
    let ema_v = ema_value[i];
    let rsi_curr = rsi_values[i];
    let rsi_prev = rsi_values[i - 1];
    let rsi_prev2 = rsi_values[i - 2];
    let atr_val = atr_values[i];
    let adx_val = adx_values[i];
    
    if atr_val == 0.0 || atr_ma == 0.0 {
        let conditions = vec![
            SignalCondition::new("ATR Calculation", false, "Zero/Invalid")
                .with_target("Non-zero ATR required")
                .with_description("Waiting for price movement data"),
        ];
        return SignalResult::hold_with_conditions("ATR is zero - insufficient volatility data", conditions, "NEUTRAL");
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // REGIME FILTERS
    // ═══════════════════════════════════════════════════════════════════
    
    // Filter 1: Trend direction
    let is_uptrend = price > ema_t;
    let trend_strength = ((price - ema_t) / ema_t * 100.0).abs();
    
    // Filter 2: ADX - Is the market actually trending?
    let is_trending = adx_val >= adx_threshold;
    
    // Filter 3: Volatility regime - Avoid news spikes and dead markets
    let vol_ratio = atr_val / atr_ma;
    let is_normal_volatility = vol_ratio > 0.5 && vol_ratio < vol_filter_mult;
    
    // Filter 4: Value zone
    let distance_from_value = (price - ema_v).abs();
    let is_in_value_zone = distance_from_value <= atr_val * atr_distance_mult;
    
    // Filter 5: RSI momentum turning
    let rsi_turning_up = rsi_curr > rsi_prev && rsi_prev <= rsi_prev2;
    let rsi_turning_down = rsi_curr < rsi_prev && rsi_prev >= rsi_prev2;
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STATUS MESSAGE
    // ═══════════════════════════════════════════════════════════════════
    let regime_status = format!(
        "Trend: {} | ADX: {:.0} {} | Vol: {:.1}x {}",
        if is_uptrend { "BULL" } else { "BEAR" },
        adx_val,
        if is_trending { "✓" } else { "(weak)" },
        vol_ratio,
        if is_normal_volatility { "✓" } else { "(abnormal)" }
    );
    
    let momentum_status = if is_uptrend {
        if rsi_turning_up { "Turn: ✓" } else { "Turn: waiting" }
    } else {
        if rsi_turning_down { "Turn: ✓" } else { "Turn: waiting" }
    };
    
    let signal_status = format!(
        "RSI: {:.1} {} | Zone: {:.1} ATR {}",
        rsi_curr,
        momentum_status,
        distance_from_value / atr_val,
        if is_in_value_zone { "✓" } else { "✗" }
    );
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS (always, for frontend display)
    // ═══════════════════════════════════════════════════════════════════
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    
    // Show MRATE boost status in conditions
    let mrate_label = if mrate_active {
        format!(" 🚀 ({:.0}%)", (1.0 - relaxation) * 100.0)
    } else {
        "".to_string()
    };
    
    let mut conditions = vec![
        SignalCondition::new(
            if is_uptrend { "Uptrend (Price > EMA)" } else { "Downtrend (Price < EMA)" },
            true,
            if is_uptrend { "BULL" } else { "BEAR" }
        ),
        SignalCondition::new(
            &format!("ADX >= {:.0} (Trending){}", adx_threshold, mrate_label),
            is_trending,
            &format!("{:.0}", adx_val)
        ).with_target(&format!(">= {:.0}", adx_threshold)),
        SignalCondition::new(
            "Normal Volatility",
            is_normal_volatility,
            &format!("{:.1}x", vol_ratio)
        ).with_target("0.5-2.0x"),
    ];
    
    // RSI condition with MRATE-relaxed threshold
    if is_uptrend {
        conditions.push(
            SignalCondition::new(
                &format!("RSI Oversold (< {:.0}){}", rsi_oversold, mrate_label),
                rsi_curr < rsi_oversold,
                &format!("{:.1}", rsi_curr)
            ).with_target(&format!("< {:.0}", rsi_oversold))
        );
    } else {
        conditions.push(
            SignalCondition::new(
                &format!("RSI Overbought (> {:.0}){}", rsi_overbought, mrate_label),
                rsi_curr > rsi_overbought,
                &format!("{:.1}", rsi_curr)
            ).with_target(&format!(">{:.0}", rsi_overbought))
        );
    }
    
    // Value zone condition
    let zone_value = if is_in_value_zone { 
        "✓".to_string() 
    } else { 
        format!("{:.1} ATR", distance_from_value / atr_val) 
    };
    conditions.push(
        SignalCondition::new(
            "In Value Zone",
            is_in_value_zone,
            &zone_value
        ).with_target(&format!("< {:.1} ATR", atr_distance_mult))
    );
    
    // Momentum turn condition
    let is_turning = if is_uptrend { rsi_turning_up } else { rsi_turning_down };
    conditions.push(
        SignalCondition::new(
            if is_uptrend { "RSI Turning Up" } else { "RSI Turning Down" },
            is_turning,
            if is_turning { "Confirmed" } else { "Waiting..." }
        ).with_target("Momentum shift")
    );
    
    // ═══════════════════════════════════════════════════════════════════
    // REGIME REJECTION - Don't trade in bad conditions (but return conditions)
    // ═══════════════════════════════════════════════════════════════════
    if !is_trending {
        return SignalResult::hold_with_conditions(
            &format!("{} | ADX {:.0} < {:.0} - choppy market", regime_status, adx_val, adx_threshold),
            conditions,
            direction
        );
    }
    
    if !is_normal_volatility {
        let reason = if vol_ratio >= vol_filter_mult {
            format!("{} | Volatility spike ({:.1}x) - likely news", regime_status, vol_ratio)
        } else {
            format!("{} | Low volatility ({:.1}x) - dead market", regime_status, vol_ratio)
        };
        return SignalResult::hold_with_conditions(&reason, conditions, direction);
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // LONG SETUP - All filters must pass
    // ═══════════════════════════════════════════════════════════════════
    if is_uptrend && rsi_curr < rsi_oversold && is_in_value_zone && rsi_turning_up {
        let trend_conf = (trend_strength / 2.0).min(0.15);
        let rsi_conf = ((rsi_oversold - rsi_curr) / 20.0).min(0.15);
        let adx_conf = ((adx_val - adx_threshold) / 30.0).min(0.1);
        let confidence = 0.55 + trend_conf + rsi_conf + adx_conf;
        
        let mrate_note = if mrate_active { " 🚀 MRATE BOOST" } else { "" };
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!(
                "LONG: {} | RSI {:.1} < {:.0} ✓ | ADX {:.0}{}",
                regime_status, rsi_curr, rsi_oversold, adx_val, mrate_note
            ),
            conditions
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // SHORT SETUP - All filters must pass
    // ═══════════════════════════════════════════════════════════════════
    if !is_uptrend && rsi_curr > rsi_overbought && is_in_value_zone && rsi_turning_down {
        let trend_conf = (trend_strength / 2.0).min(0.15);
        let rsi_conf = ((rsi_curr - rsi_overbought) / 20.0).min(0.15);
        let adx_conf = ((adx_val - adx_threshold) / 30.0).min(0.1);
        let confidence = 0.55 + trend_conf + rsi_conf + adx_conf;
        
        let mrate_note = if mrate_active { " 🚀 MRATE BOOST" } else { "" };
        return SignalResult::sell_with_conditions(
            confidence.min(0.85),
            &format!(
                "SHORT: {} | RSI {:.1} > {:.0} ✓ | ADX {:.0}{}",
                regime_status, rsi_curr, rsi_overbought, adx_val, mrate_note
            ),
            conditions
        );
    }
    
    // Default: return hold with all conditions for display
    SignalResult::hold_with_conditions(
        &format!("{} | {}", regime_status, signal_status),
        conditions,
        direction
    )
}

/// London/NY Gold Trend Continuation System
/// 
/// A professional multi-filter strategy optimized for gold trading:
/// 1. Session filter: Only trade London/NY hours (07:00-16:00 UTC)
/// 2. Trend filter: EMA200 + ADX on higher timeframe logic
/// 3. Volatility regime: ATR filter to avoid news/dead markets
/// 4. Pullback zone: Price between EMA21-EMA50, within 1.2 ATR
/// 5. Momentum trigger: RSI cross + candle breakout confirmation
fn london_ny_trend_continuation_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    // MRATE relaxation (applied to RSI triggers and ADX)
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let _mrate_active = relaxation < 1.0;
    
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let session_start = params.get("session_start").and_then(|v| v.as_u64()).unwrap_or(7) as u32;
    let session_end = params.get("session_end").and_then(|v| v.as_u64()).unwrap_or(16) as u32;
    let trend_ema_period = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let fast_ema_period = params.get("fast_ema").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    let slow_ema_period = params.get("slow_ema").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let adx_threshold = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let atr_zone_mult = params.get("atr_zone_mult").and_then(|v| v.as_f64()).unwrap_or(1.2);
    let vol_low = params.get("vol_low").and_then(|v| v.as_f64()).unwrap_or(0.7);
    let vol_high = params.get("vol_high").and_then(|v| v.as_f64()).unwrap_or(1.8);
    let rsi_long_trigger = params.get("rsi_long_trigger").and_then(|v| v.as_f64()).unwrap_or(45.0);
    let rsi_short_trigger = params.get("rsi_short_trigger").and_then(|v| v.as_f64()).unwrap_or(55.0);
    
    let i = candles.len() - 1;
    if i < 3 {
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/4 candles", i + 1))
                .with_target("4+ candles required")
                .with_description("Collecting initial price data"),
        ];
        return SignalResult::hold_with_conditions("Insufficient candles for analysis", conditions, "NEUTRAL");
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: SESSION FILTER (use actual current time, not candle time)
    // ═══════════════════════════════════════════════════════════════════
    let current_hour = Utc::now().hour();
    let in_trading_session = current_hour >= session_start && current_hour < session_end;
    
    if !in_trading_session {
        let conditions = vec![
            SignalCondition::new("Trading Session", false, &format!("{:02}:00 UTC", current_hour))
                .with_target(&format!("{:02}:00-{:02}:00 UTC", session_start, session_end))
                .with_description("Outside London/NY trading hours"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("⏰ Outside trading hours ({:02}:00 UTC)", current_hour),
            conditions,
            "WAIT"
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE ALL INDICATORS
    // ═══════════════════════════════════════════════════════════════════
    let ema_trend = ema(candles, trend_ema_period);
    let ema_fast = ema(candles, fast_ema_period);
    let ema_slow = ema(candles, slow_ema_period);
    let rsi_values = rsi(candles, rsi_period);
    let atr_values = atr(candles, atr_period);
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    
    if i >= ema_trend.len() || i >= ema_fast.len() || i >= rsi_values.len() || i >= atr_values.len() || i >= adx_values.len() {
        let progress = (i as f64 / trend_ema_period as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, trend_ema_period))
                .with_target(&format!("{} candles required", trend_ema_period))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("EMA Trend", false, "Pending")
                .with_target(&format!("EMA({})", trend_ema_period)),
            SignalCondition::new("RSI Momentum", false, "Pending")
                .with_target("Momentum indicator"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Calculating indicators ({:.0}% complete)...", progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let price = candles[i].close;
    let ema_t = ema_trend[i];
    let ema_f = ema_fast[i];
    let ema_s = ema_slow[i];
    let rsi_curr = rsi_values[i];
    let rsi_prev = rsi_values[i - 1];
    let atr_val = atr_values[i];
    let adx_val = adx_values[i];
    
    // ═══════════════════════════════════════════════════════════════════
    // COMPUTE ALL CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let is_uptrend = price > ema_t;
    let is_trending = adx_val > adx_threshold;
    
    let atr_ma: f64 = if i >= 30 {
        atr_values[i.saturating_sub(30)..i].iter().sum::<f64>() / 30.0
    } else {
        atr_values[..i].iter().sum::<f64>() / i.max(1) as f64
    };
    let vol_ratio = if atr_ma > 0.0 { atr_val / atr_ma } else { 1.0 };
    let is_normal_vol = vol_ratio >= vol_low && vol_ratio <= vol_high;
    
    let in_value_zone = price >= ema_s.min(ema_f) && price <= ema_s.max(ema_f);
    let distance_from_ema = (price - ema_f).abs();
    let atr_distance = if atr_val > 0.0 { distance_from_ema / atr_val } else { 999.0 };
    let within_atr_zone = atr_distance <= atr_zone_mult;
    let zone_ok = in_value_zone || within_atr_zone;
    
    let rsi_long_cross = rsi_prev < rsi_long_trigger && rsi_curr >= rsi_long_trigger;
    let rsi_short_cross = rsi_prev > rsi_short_trigger && rsi_curr <= rsi_short_trigger;
    
    let prev_high = candles[i - 1].high;
    let prev_low = candles[i - 1].low;
    let curr_close = candles[i].close;
    let curr_open = candles[i].open;
    let bullish_candle = curr_close > curr_open && curr_close > prev_high;
    let bearish_candle = curr_close < curr_open && curr_close < prev_low;
    
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    
    // Pre-compute description strings for RSI conditions to satisfy borrow checker
    let rsi_long_desc = if rsi_long_cross {
        "RSI crossed up - momentum confirmed".to_string()
    } else {
        format!("Need prev RSI < {:.0} then current >= {:.0}", rsi_long_trigger, rsi_long_trigger)
    };
    let rsi_short_desc = if rsi_short_cross {
        "RSI crossed down - momentum confirmed".to_string()
    } else {
        format!("Need prev RSI > {:.0} then current <= {:.0}", rsi_short_trigger, rsi_short_trigger)
    };
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let mut conditions = vec![
        SignalCondition::new("Trading Session", true, &format!("{:02}:00 UTC", current_hour))
            .with_target(&format!("{:02}:00-{:02}:00", session_start, session_end))
            .with_description("Inside London/NY hours"),
        
        SignalCondition::new("Trend Direction", true, if is_uptrend { "BULL" } else { "BEAR" })
            .with_target(&format!("Price {} EMA{}", if is_uptrend { ">" } else { "<" }, trend_ema_period))
            .with_description(&format!("Price {:.2} vs EMA {:.2}", price, ema_t)),
        
        SignalCondition::new("ADX Trend Strength", is_trending, &format!("{:.0}", adx_val))
            .with_target(&format!("> {:.0}", adx_threshold))
            .with_description(if is_trending { "Market is trending" } else { "Market is choppy" }),
        
        SignalCondition::new("Volatility Regime", is_normal_vol, &format!("{:.2}x avg", vol_ratio))
            .with_target(&format!("{:.1}x - {:.1}x", vol_low, vol_high))
            .with_description(if !is_normal_vol && vol_ratio > vol_high { "Volatility spike - possible news" } 
                              else if !is_normal_vol { "Low volatility - dead market" } 
                              else { "Normal volatility" }),
    ];
    
    // Add direction-specific conditions
    if is_uptrend {
        conditions.push(
            SignalCondition::new("Pullback Zone", zone_ok, &format!("{:.1} ATR from EMA21", atr_distance))
                .with_target(&format!("< {:.1} ATR", atr_zone_mult))
                .with_description(if zone_ok { "Price in value zone" } else { "Waiting for pullback" })
        );
        conditions.push(
            SignalCondition::new("RSI Momentum", rsi_long_cross, &format!("{:.1} (prev: {:.1})", rsi_curr, rsi_prev))
                .with_target(&format!("Cross up through {:.0}", rsi_long_trigger))
                .with_description(&rsi_long_desc)
        );
        conditions.push(
            SignalCondition::new("Bullish Candle", bullish_candle, if bullish_candle { "Confirmed" } else { "Waiting" })
                .with_target("Close > prev high")
                .with_description(if bullish_candle { "Breakout candle formed" } else { "Need bullish breakout candle" })
        );
    } else {
        conditions.push(
            SignalCondition::new("Rally Zone", zone_ok, &format!("{:.1} ATR from EMA21", atr_distance))
                .with_target(&format!("< {:.1} ATR", atr_zone_mult))
                .with_description(if zone_ok { "Price in value zone" } else { "Waiting for rally" })
        );
        conditions.push(
            SignalCondition::new("RSI Momentum", rsi_short_cross, &format!("{:.1} (prev: {:.1})", rsi_curr, rsi_prev))
                .with_target(&format!("Cross down through {:.0}", rsi_short_trigger))
                .with_description(&rsi_short_desc)
        );
        conditions.push(
            SignalCondition::new("Bearish Candle", bearish_candle, if bearish_candle { "Confirmed" } else { "Waiting" })
                .with_target("Close < prev low")
                .with_description(if bearish_candle { "Breakdown candle formed" } else { "Need bearish breakdown candle" })
        );
    }
    
    // Build status message for backward compatibility
    let status = format!(
        "Session: ✓ | Trend: {} | ADX: {:.0} {} | Vol: {:.1}x {} | Zone: {}",
        if is_uptrend { "BULL" } else { "BEAR" },
        adx_val, if is_trending { "✓" } else { "✗" },
        vol_ratio, if is_normal_vol { "✓" } else { "✗" },
        if zone_ok { "✓" } else { "✗" }
    );
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    
    // LONG SIGNAL
    if is_uptrend && is_trending && is_normal_vol && zone_ok && rsi_long_cross && bullish_candle {
        let confidence = 0.60 + ((adx_val - adx_threshold) / 30.0).min(0.15) + ((rsi_curr - 40.0) / 20.0).min(0.1);
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!("🟢 LONG TRIGGER | {} | RSI crossed {:.0} ↑", status, rsi_long_trigger),
            conditions
        );
    }
    
    // SHORT SIGNAL
    if !is_uptrend && is_trending && is_normal_vol && zone_ok && rsi_short_cross && bearish_candle {
        let confidence = 0.60 + ((adx_val - adx_threshold) / 30.0).min(0.15) + ((60.0 - rsi_curr) / 20.0).min(0.1);
        return SignalResult::sell_with_conditions(
            confidence.min(0.85),
            &format!("🔴 SHORT TRIGGER | {} | RSI crossed {:.0} ↓", status, rsi_short_trigger),
            conditions
        );
    }
    
    // HOLD with conditions
    SignalResult::hold_with_conditions(&format!("{} | RSI: {:.1}", status, rsi_curr), conditions, direction)
}

/// London/NY Gold Trend 2.0
/// 
/// Enhanced version with tighter risk management and additional confirmations:
/// 1. Session filter: Only trade London/NY hours (07:00-16:00 UTC)
/// 2. Trend filter: EMA200 + ADX confirms trending market
/// 3. Volatility regime: ATR filter to avoid news/dead markets
/// 4. Pullback zone: Price within 1.5 ATR of EMA21 (widened from 1.2)
/// 5. RSI momentum: RSI cross through trigger level
/// 6. MACD confirmation: Histogram must agree with direction (NEW)
/// 7. Candle confirmation: Breakout candle required
/// 8. Dynamic ATR-based SL/TP (NEW)
/// 9. Minimum R:R ratio check (NEW)
fn london_ny_trend_2_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    // MRATE relaxation
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let mrate_active = relaxation < 1.0;
    
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let session_start = params.get("session_start").and_then(|v| v.as_u64()).unwrap_or(7) as u32;
    let session_end = params.get("session_end").and_then(|v| v.as_u64()).unwrap_or(16) as u32;
    let trend_ema_period = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let fast_ema_period = params.get("fast_ema").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    let slow_ema_period = params.get("slow_ema").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let adx_threshold_base = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let adx_threshold = adx_threshold_base * relaxation;
    let atr_zone_mult = params.get("atr_zone_mult").and_then(|v| v.as_f64()).unwrap_or(1.5);  // Widened from 1.2
    let vol_low = params.get("vol_low").and_then(|v| v.as_f64()).unwrap_or(0.6);
    let vol_high = params.get("vol_high").and_then(|v| v.as_f64()).unwrap_or(2.0);  // Widened from 1.8
    let rsi_long_trigger = params.get("rsi_long_trigger").and_then(|v| v.as_f64()).unwrap_or(45.0);
    let rsi_short_trigger = params.get("rsi_short_trigger").and_then(|v| v.as_f64()).unwrap_or(55.0);
    
    // NEW: Risk management parameters
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(2.0);  // Stop loss
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(3.0);  // Take profit
    let min_rr_ratio = params.get("min_rr_ratio").and_then(|v| v.as_f64()).unwrap_or(1.5);  // Minimum R:R
    
    // NEW: MACD parameters
    let macd_fast = params.get("macd_fast").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
    let macd_slow = params.get("macd_slow").and_then(|v| v.as_u64()).unwrap_or(26) as usize;
    let macd_signal = params.get("macd_signal").and_then(|v| v.as_u64()).unwrap_or(9) as usize;
    
    let i = candles.len() - 1;
    if i < 3 {
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/4 candles", i + 1))
                .with_target("4+ candles required")
                .with_description("Collecting initial price data"),
        ];
        return SignalResult::hold_with_conditions("Insufficient candles for analysis", conditions, "NEUTRAL");
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: SESSION FILTER
    // ═══════════════════════════════════════════════════════════════════
    let current_hour = Utc::now().hour();
    let in_trading_session = current_hour >= session_start && current_hour < session_end;
    
    if !in_trading_session {
        let conditions = vec![
            SignalCondition::new("Trading Session", false, &format!("{:02}:00 UTC", current_hour))
                .with_target(&format!("{:02}:00-{:02}:00 UTC", session_start, session_end))
                .with_description("Outside London/NY trading hours"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("⏰ Outside trading hours ({:02}:00 UTC)", current_hour),
            conditions,
            "WAIT"
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE ALL INDICATORS
    // ═══════════════════════════════════════════════════════════════════
    let ema_trend = ema(candles, trend_ema_period);
    let ema_fast = ema(candles, fast_ema_period);
    let ema_slow = ema(candles, slow_ema_period);
    let rsi_values = rsi(candles, rsi_period);
    let atr_values = atr(candles, atr_period);
    let (adx_values, plus_di, minus_di) = adx(candles, 14);
    let (macd_line, signal_line, histogram) = macd(candles, macd_fast, macd_slow, macd_signal);
    
    if i >= ema_trend.len() || i >= ema_fast.len() || i >= rsi_values.len() || 
       i >= atr_values.len() || i >= adx_values.len() || i >= histogram.len() {
        let progress = (i as f64 / trend_ema_period as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, trend_ema_period))
                .with_target(&format!("{} candles required", trend_ema_period))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("EMA Trend", false, "Pending")
                .with_target(&format!("EMA({})", trend_ema_period)),
            SignalCondition::new("MACD Confirmation", false, "Pending")
                .with_target("Histogram direction"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Calculating indicators ({:.0}% complete)...", progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let price = candles[i].close;
    let ema_t = ema_trend[i];
    let ema_f = ema_fast[i];
    let ema_s = ema_slow[i];
    let rsi_curr = rsi_values[i];
    let rsi_prev = rsi_values[i - 1];
    let atr_val = atr_values[i];
    let adx_val = adx_values[i];
    let macd_hist = histogram[i];
    let macd_hist_prev = histogram[i - 1];
    
    // ═══════════════════════════════════════════════════════════════════
    // COMPUTE ALL CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let is_uptrend = price > ema_t;
    let is_trending = adx_val > adx_threshold;
    
    // Volatility regime
    let atr_ma: f64 = if i >= 30 {
        atr_values[i.saturating_sub(30)..i].iter().sum::<f64>() / 30.0
    } else {
        atr_values[..i].iter().sum::<f64>() / i.max(1) as f64
    };
    let vol_ratio = if atr_ma > 0.0 { atr_val / atr_ma } else { 1.0 };
    let is_normal_vol = vol_ratio >= vol_low && vol_ratio <= vol_high;
    
    // Pullback zone (widened)
    let distance_from_ema = (price - ema_f).abs();
    let atr_distance = if atr_val > 0.0 { distance_from_ema / atr_val } else { 999.0 };
    let zone_ok = atr_distance <= atr_zone_mult;
    
    // RSI momentum
    let rsi_long_cross = rsi_prev < rsi_long_trigger && rsi_curr >= rsi_long_trigger;
    let rsi_short_cross = rsi_prev > rsi_short_trigger && rsi_curr <= rsi_short_trigger;
    
    // NEW: MACD confirmation
    let macd_bullish = macd_hist > 0.0 || (macd_hist > macd_hist_prev && macd_hist_prev < 0.0);  // Positive or turning up
    let macd_bearish = macd_hist < 0.0 || (macd_hist < macd_hist_prev && macd_hist_prev > 0.0);  // Negative or turning down
    
    // Candle confirmation
    let prev_high = candles[i - 1].high;
    let prev_low = candles[i - 1].low;
    let curr_close = candles[i].close;
    let curr_open = candles[i].open;
    let bullish_candle = curr_close > curr_open && curr_close > prev_high;
    let bearish_candle = curr_close < curr_open && curr_close < prev_low;
    
    // NEW: Calculate dynamic SL/TP
    let stop_loss = atr_val * atr_sl_mult;
    let take_profit = atr_val * atr_tp_mult;
    let actual_rr = take_profit / stop_loss;
    let rr_ok = actual_rr >= min_rr_ratio;
    
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let mrate_label = if mrate_active { format!(" 🚀 ({:.0}%)", (1.0 - relaxation) * 100.0) } else { "".to_string() };
    
    let mut conditions = vec![
        SignalCondition::new("Trading Session", true, &format!("{:02}:00 UTC", current_hour))
            .with_target(&format!("{:02}:00-{:02}:00", session_start, session_end))
            .with_description("Inside London/NY hours"),
        
        SignalCondition::new("Trend Direction", true, if is_uptrend { "BULL ↑" } else { "BEAR ↓" })
            .with_target(&format!("Price {} EMA{}", if is_uptrend { ">" } else { "<" }, trend_ema_period))
            .with_description(&format!("Price ${:.2} vs EMA ${:.2}", price, ema_t)),
        
        SignalCondition::new(&format!("ADX Trend Strength{}", mrate_label), is_trending, &format!("{:.0}", adx_val))
            .with_target(&format!("> {:.0}", adx_threshold))
            .with_description(if is_trending { "Strong trend" } else { "Choppy - avoid" }),
        
        SignalCondition::new("Volatility Regime", is_normal_vol, &format!("{:.2}x avg", vol_ratio))
            .with_target(&format!("{:.1}x - {:.1}x", vol_low, vol_high))
            .with_description(if !is_normal_vol && vol_ratio > vol_high { "⚠️ Volatility spike" } 
                              else if !is_normal_vol { "⚠️ Dead market" } 
                              else { "Normal volatility" }),
        
        SignalCondition::new("Pullback Zone", zone_ok, &format!("{:.1} ATR", atr_distance))
            .with_target(&format!("< {:.1} ATR", atr_zone_mult))
            .with_description(if zone_ok { "In value zone" } else { "Too extended" }),
    ];
    
    // Direction-specific conditions
    if is_uptrend {
        conditions.push(
            SignalCondition::new("RSI Momentum", rsi_long_cross, &format!("{:.1} (prev: {:.1})", rsi_curr, rsi_prev))
                .with_target(&format!("Cross up through {:.0}", rsi_long_trigger))
                .with_description(if rsi_long_cross { "✓ Momentum confirmed" } else { "Waiting for cross" })
        );
        conditions.push(
            SignalCondition::new("MACD Confirmation", macd_bullish, &format!("{:.4}", macd_hist))
                .with_target("Histogram > 0 or turning up")
                .with_description(if macd_bullish { "✓ MACD supports long" } else { "MACD bearish - skip" })
        );
        conditions.push(
            SignalCondition::new("Bullish Candle", bullish_candle, if bullish_candle { "✓ Confirmed" } else { "Waiting" })
                .with_target("Close > prev high")
                .with_description(if bullish_candle { "Breakout candle" } else { "Need bullish candle" })
        );
    } else {
        conditions.push(
            SignalCondition::new("RSI Momentum", rsi_short_cross, &format!("{:.1} (prev: {:.1})", rsi_curr, rsi_prev))
                .with_target(&format!("Cross down through {:.0}", rsi_short_trigger))
                .with_description(if rsi_short_cross { "✓ Momentum confirmed" } else { "Waiting for cross" })
        );
        conditions.push(
            SignalCondition::new("MACD Confirmation", macd_bearish, &format!("{:.4}", macd_hist))
                .with_target("Histogram < 0 or turning down")
                .with_description(if macd_bearish { "✓ MACD supports short" } else { "MACD bullish - skip" })
        );
        conditions.push(
            SignalCondition::new("Bearish Candle", bearish_candle, if bearish_candle { "✓ Confirmed" } else { "Waiting" })
                .with_target("Close < prev low")
                .with_description(if bearish_candle { "Breakdown candle" } else { "Need bearish candle" })
        );
    }
    
    // Risk management conditions
    conditions.push(
        SignalCondition::new("Risk:Reward", rr_ok, &format!("{:.1}:1", actual_rr))
            .with_target(&format!("≥ {:.1}:1", min_rr_ratio))
            .with_description(&format!("SL: ${:.2}, TP: ${:.2}", stop_loss, take_profit))
    );
    
    // Status message
    let status = format!(
        "Trend: {} | ADX: {:.0} {} | Vol: {:.1}x {} | Zone: {} | MACD: {}",
        if is_uptrend { "BULL" } else { "BEAR" },
        adx_val, if is_trending { "✓" } else { "✗" },
        vol_ratio, if is_normal_vol { "✓" } else { "✗" },
        if zone_ok { "✓" } else { "✗" },
        if (is_uptrend && macd_bullish) || (!is_uptrend && macd_bearish) { "✓" } else { "✗" }
    );
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    
    // LONG SIGNAL - All 7 conditions must pass
    if is_uptrend && is_trending && is_normal_vol && zone_ok && 
       rsi_long_cross && macd_bullish && bullish_candle && rr_ok {
        let adx_bonus = ((adx_val - adx_threshold) / 30.0).min(0.1);
        let rsi_bonus = ((rsi_curr - 40.0) / 30.0).min(0.1);
        let confidence = (0.65 + adx_bonus + rsi_bonus).min(0.85);
        
        let mrate_note = if mrate_active { " 🚀" } else { "" };
        return SignalResult::buy_with_conditions(
            confidence,
            &format!("🟢 LONG{} | {} | SL: ${:.2} TP: ${:.2} (R:R {:.1}:1)", 
                mrate_note, status, stop_loss, take_profit, actual_rr),
            conditions
        );
    }
    
    // SHORT SIGNAL - All 7 conditions must pass
    if !is_uptrend && is_trending && is_normal_vol && zone_ok && 
       rsi_short_cross && macd_bearish && bearish_candle && rr_ok {
        let adx_bonus = ((adx_val - adx_threshold) / 30.0).min(0.1);
        let rsi_bonus = ((60.0 - rsi_curr) / 30.0).min(0.1);
        let confidence = (0.65 + adx_bonus + rsi_bonus).min(0.85);
        
        let mrate_note = if mrate_active { " 🚀" } else { "" };
        return SignalResult::sell_with_conditions(
            confidence,
            &format!("🔴 SHORT{} | {} | SL: ${:.2} TP: ${:.2} (R:R {:.1}:1)", 
                mrate_note, status, stop_loss, take_profit, actual_rr),
            conditions
        );
    }
    
    // HOLD with all conditions
    SignalResult::hold_with_conditions(
        &format!("⏳ {} | RSI: {:.1} | Waiting for setup", status, rsi_curr),
        conditions,
        direction
    )
}

/// Forecast Confidence Strategy (Daily Gold Swing Trading)
/// 
/// Based on the CUHK Holt-Winters trading paper:
/// 1. Use Holt-Winters to predict next day's close
/// 2. Track rolling 30-day model accuracy
/// 3. RSI agreement filter for confirmation
/// 4. Dynamic position sizing based on model confidence ("trading shrink ratio")
/// 5. ATR-based stops/targets, max 5 day hold
/// 6. MRATE integration: When regime favors mean reversion, relax RSI agreement requirement
fn forecast_confidence_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    // MRATE relaxation - when weight is high (favorable regime), relax RSI requirement
    let relaxation = calculate_mrate_relaxation(mrate_weight);
    let mrate_active = relaxation < 1.0;
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let alpha = params.get("hw_alpha").and_then(|v| v.as_f64()).unwrap_or(0.3);  // Level smoothing
    let beta = params.get("hw_beta").and_then(|v| v.as_f64()).unwrap_or(0.1);    // Trend smoothing
    let forecast_threshold = params.get("forecast_threshold").and_then(|v| v.as_f64()).unwrap_or(0.3);  // % edge required
    let accuracy_window = params.get("accuracy_window").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
    let min_accuracy = params.get("min_accuracy").and_then(|v| v.as_f64()).unwrap_or(55.0);  // % minimum to trade
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(1.8);  // Stop loss ATR multiplier
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(3.6);  // Take profit ATR multiplier
    
    let n = candles.len();
    // Need enough data for Holt-Winters initialization + accuracy window
    let min_candles = accuracy_window + 30;
    if n < min_candles {
        let conditions = vec![
            SignalCondition::new(
                "Historical Data",
                false,
                &format!("{} candles", n)
            ).with_target(&format!(">= {} daily candles", min_candles))
             .with_description(&format!("Holt-Winters needs {} days of history to build forecast model", min_candles)),
            SignalCondition::new("HW Forecast", false, "Waiting...").with_target("Model not ready"),
            SignalCondition::new("Model Accuracy", false, "—").with_target(&format!("> {:.0}%", min_accuracy)),
            SignalCondition::new("RSI Agreement", false, "—").with_target("Pending forecast"),
            SignalCondition::new("Position Size", false, "0%").with_target("Needs model"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("Collecting data: {}/{} daily candles", n, min_candles),
            conditions,
            "NEUTRAL"
        );
    }
    
    let i = n - 1;
    let price = candles[i].close;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: HOLT-WINTERS EXPONENTIAL SMOOTHING FORECAST
    // ═══════════════════════════════════════════════════════════════════
    // Initialize level and trend from first few observations
    let mut level = candles[0].close;
    let mut trend = (candles[4].close - candles[0].close) / 4.0;  // Initial trend estimate
    
    // Run Holt-Winters through all candles to get current state
    for j in 1..n {
        let y = candles[j].close;
        let prev_level = level;
        // Level equation: e_t = α*y_t + (1-α)*(e_{t-1} + b_{t-1})
        level = alpha * y + (1.0 - alpha) * (level + trend);
        // Trend equation: b_t = β*(e_t - e_{t-1}) + (1-β)*b_{t-1}
        trend = beta * (level - prev_level) + (1.0 - beta) * trend;
    }
    
    // Forecast for tomorrow (h=1): ŷ_{T+1} = e_T + 1*b_T
    let predicted_price = level + trend;
    let forecast_edge = ((predicted_price - price) / price) * 100.0;  // As percentage
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: CONVERT FORECAST TO DIRECTION
    // ═══════════════════════════════════════════════════════════════════
    let bullish_forecast = forecast_edge > forecast_threshold;
    let bearish_forecast = forecast_edge < -forecast_threshold;
    let has_signal = bullish_forecast || bearish_forecast;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: ROLLING ACCURACY CALCULATION (30-day backcasting)
    // ═══════════════════════════════════════════════════════════════════
    // Simulate past predictions to calculate accuracy
    let mut correct_predictions = 0;
    let mut total_predictions = 0;
    
    // Re-run Holt-Winters for accuracy window to backtest predictions
    let start_idx = n.saturating_sub(accuracy_window + 1);
    let mut hist_level = candles[0].close;
    let mut hist_trend = (candles[4.min(n-1)].close - candles[0].close) / 4.0;
    
    for j in 1..start_idx {
        let y = candles[j].close;
        let prev_level = hist_level;
        hist_level = alpha * y + (1.0 - alpha) * (hist_level + hist_trend);
        hist_trend = beta * (hist_level - prev_level) + (1.0 - beta) * hist_trend;
    }
    
    // Now track predictions for accuracy window
    for j in start_idx..n {
        // Forecast for next day
        let forecast = hist_level + hist_trend;
        let actual_prev = candles[j - 1].close;
        let actual_curr = candles[j].close;
        
        // Did we predict direction correctly?
        let predicted_up = forecast > actual_prev;
        let actual_up = actual_curr > actual_prev;
        
        if predicted_up == actual_up {
            correct_predictions += 1;
        }
        total_predictions += 1;
        
        // Update model with actual
        let prev_level = hist_level;
        hist_level = alpha * actual_curr + (1.0 - alpha) * (hist_level + hist_trend);
        hist_trend = beta * (hist_level - prev_level) + (1.0 - beta) * hist_trend;
    }
    
    let model_accuracy = if total_predictions > 0 {
        (correct_predictions as f64 / total_predictions as f64) * 100.0
    } else {
        50.0
    };
    
    let accuracy_ok = model_accuracy >= min_accuracy;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: RSI AGREEMENT FILTER (with MRATE relaxation)
    // ═══════════════════════════════════════════════════════════════════
    let rsi_values = rsi(candles, rsi_period);
    let rsi_curr = if i < rsi_values.len() { rsi_values[i] } else { 50.0 };
    
    // MRATE relaxation: when regime favors mean reversion, soften RSI threshold
    // Normal: need RSI > 50 for long, < 50 for short
    // Relaxed: accept RSI > 45 for long, < 55 for short (when MRATE weight >= 70%)
    let rsi_long_threshold = 50.0 - (10.0 * (1.0 - relaxation));   // 50 -> 40 when fully relaxed
    let rsi_short_threshold = 50.0 + (10.0 * (1.0 - relaxation));  // 50 -> 60 when fully relaxed
    
    // Agreement logic from paper, with MRATE relaxation
    let rsi_agrees = if bullish_forecast {
        rsi_curr > rsi_long_threshold  // Bullish: forecast UP AND RSI > threshold
    } else if bearish_forecast {
        rsi_curr < rsi_short_threshold  // Bearish: forecast DOWN AND RSI < threshold
    } else {
        false
    };
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: DYNAMIC POSITION SIZING (Trading Shrink Ratio)
    // ═══════════════════════════════════════════════════════════════════
    // Convert accuracy to risk level (from paper's formulas)
    let base_risk = if model_accuracy < 55.0 {
        0.0  // No trading
    } else if model_accuracy < 60.0 {
        0.25  // 0.25% risk
    } else if model_accuracy < 65.0 {
        0.5   // 0.5% risk
    } else if model_accuracy < 70.0 {
        0.75  // 0.75% risk
    } else {
        1.0   // 1% risk
    };
    
    // If RSI disagrees, cut size in half (from paper)
    let adjusted_risk = if rsi_agrees { base_risk } else { base_risk * 0.5 };
    
    // ═══════════════════════════════════════════════════════════════════
    // CALCULATE ATR FOR STOPS/TARGETS
    // ═══════════════════════════════════════════════════════════════════
    let atr_values = atr(candles, atr_period);
    let atr_val = if i < atr_values.len() { atr_values[i] } else { 0.0 };
    let stop_loss = atr_val * atr_sl_mult;
    let take_profit = atr_val * atr_tp_mult;
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════════
    let direction = if bullish_forecast { "LONG" } else if bearish_forecast { "SHORT" } else { "NEUTRAL" };
    
    // Pre-compute description strings to avoid borrow issues
    let forecast_desc = if has_signal {
        format!("Model predicts {} move", if bullish_forecast { "upward" } else { "downward" })
    } else {
        "Edge below threshold - no clear direction".to_string()
    };
    
    let position_desc = if adjusted_risk > 0.0 {
        format!("SL: ${:.2}, TP: ${:.2}", stop_loss, take_profit)
    } else {
        "Model confidence too low to trade".to_string()
    };
    
    // Pre-compute RSI strings to avoid borrow issues
    let rsi_value_str = format!("{:.1}{}", rsi_curr, if mrate_active { " \u{1F680}" } else { "" });
    let rsi_target_str = if bullish_forecast {
        format!("> {:.0}{}", rsi_long_threshold, if mrate_active { " (relaxed)" } else { "" })
    } else {
        format!("< {:.0}{}", rsi_short_threshold, if mrate_active { " (relaxed)" } else { "" })
    };
    
    let conditions = vec![
        SignalCondition::new("HW Forecast", has_signal, &format!("{:+.2}%", forecast_edge))
            .with_target(&format!("> ±{:.1}%", forecast_threshold))
            .with_description(&format!("Predicted: ${:.2}, Current: ${:.2}", predicted_price, price)),
        
        SignalCondition::new("Forecast Direction", has_signal, 
            if bullish_forecast { "BULLISH" } else if bearish_forecast { "BEARISH" } else { "NEUTRAL" })
            .with_description(&forecast_desc),
        
        SignalCondition::new("Model Accuracy", accuracy_ok, &format!("{:.1}%", model_accuracy))
            .with_target(&format!("> {:.0}%", min_accuracy))
            .with_description(&format!("{}/{} correct in last {} days", correct_predictions, total_predictions, accuracy_window)),
        
        SignalCondition::new("RSI Agreement", rsi_agrees, &rsi_value_str)
            .with_target(&rsi_target_str)
            .with_description(if rsi_agrees { 
                if mrate_active { "RSI confirms (MRATE relaxed threshold)" } else { "RSI confirms forecast direction" }
            } else { 
                "RSI diverges - reduced position size" 
            }),
        
        SignalCondition::new("Position Size", adjusted_risk > 0.0, &format!("{:.2}% risk", adjusted_risk))
            .with_target(&format!("> 0% (need >{}% accuracy)", min_accuracy))
            .with_description(&position_desc),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    let status = format!(
        "Forecast: {:+.2}% | Accuracy: {:.0}% | RSI: {:.1} {}",
        forecast_edge,
        model_accuracy,
        rsi_curr,
        if rsi_agrees { "✓" } else { "✗" }
    );
    
    // Only trade if accuracy is above minimum and we have a signal
    if !accuracy_ok || adjusted_risk <= 0.0 {
        return SignalResult::hold_with_conditions(
            &format!("⏸️ {} | Risk: {:.2}%", status, adjusted_risk),
            conditions,
            direction
        );
    }
    
    // LONG SIGNAL
    if bullish_forecast && accuracy_ok {
        let confidence = (model_accuracy / 100.0) * (if rsi_agrees { 1.0 } else { 0.7 });
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!("🟢 LONG | {} | Risk: {:.2}% | SL: ${:.2} TP: ${:.2}", 
                status, adjusted_risk, stop_loss, take_profit),
            conditions
        );
    }
    
    // SHORT SIGNAL
    if bearish_forecast && accuracy_ok {
        let confidence = (model_accuracy / 100.0) * (if rsi_agrees { 1.0 } else { 0.7 });
        return SignalResult::sell_with_conditions(
            confidence.min(0.85),
            &format!("🔴 SHORT | {} | Risk: {:.2}% | SL: ${:.2} TP: ${:.2}", 
                status, adjusted_risk, stop_loss, take_profit),
            conditions
        );
    }
    
    // No trade - edge below threshold
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | No edge above threshold", status),
        conditions,
        direction
    )
}

/// Gold Volatility Expansion System (Daily)
/// 
/// Inspired by Black-Scholes volatility concepts:
/// 1. Measure 20-day realized volatility vs 90-day baseline
/// 2. Detect volatility compression (squeeze)
/// 3. Confirm with Bollinger Band width at low
/// 4. Wait for price breakout with volume
/// 5. Wide ATR stops for big moves
fn volatility_expansion_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let vol_period = params.get("vol_period").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let baseline_period = params.get("baseline_period").and_then(|v| v.as_u64()).unwrap_or(90) as usize;
    let compression_ratio = params.get("compression_ratio").and_then(|v| v.as_f64()).unwrap_or(0.6);
    let bb_period = params.get("bb_period").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let bb_std = params.get("bb_std").and_then(|v| v.as_f64()).unwrap_or(2.0);
    let bw_lookback = params.get("bw_lookback").and_then(|v| v.as_u64()).unwrap_or(120) as usize;  // ~6 months
    let breakout_period = params.get("breakout_period").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(2.5);
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(5.0);
    let vol_confirm = params.get("vol_confirm").and_then(|v| v.as_f64()).unwrap_or(1.0);  // Volume must be > 1x average
    
    let n = candles.len();
    let min_candles = baseline_period.max(bw_lookback) + 10;
    if n < min_candles {
        let progress = (n as f64 / min_candles as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", n, min_candles))
                .with_target(&format!("{} daily candles required", min_candles))
                .with_description(&format!("{:.0}% complete — collecting historical data", progress)),
            SignalCondition::new("Volatility Compression", false, "Pending")
                .with_target(&format!("< {:.0}% of 90d baseline", compression_ratio * 100.0))
                .with_description("20d vs 90d realized volatility ratio"),
            SignalCondition::new("Bollinger Squeeze", false, "Pending")
                .with_target("Bandwidth near 6-month low")
                .with_description("Bollinger Band width at compression extreme"),
            SignalCondition::new("Breakout Detection", false, "Pending")
                .with_target("Price breaks Donchian channel")
                .with_description("Waiting for directional breakout from squeeze"),
            SignalCondition::new("Volume Confirmation", false, "Pending")
                .with_target(&format!("> {:.0}x average volume", vol_confirm))
                .with_description("Volume surge confirms breakout validity"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Initializing: {}/{} candles ({:.0}%)", n, min_candles, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let i = n - 1;
    let price = candles[i].close;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: CALCULATE REALIZED VOLATILITY (std dev of daily returns)
    // ═══════════════════════════════════════════════════════════════════
    let calc_volatility = |start: usize, end: usize| -> f64 {
        if end <= start + 1 { return 0.0; }
        let returns: Vec<f64> = (start + 1..=end)
            .filter(|&j| j < n)
            .map(|j| (candles[j].close / candles[j - 1].close).ln())
            .collect();
        if returns.is_empty() { return 0.0; }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        variance.sqrt() * (252.0_f64).sqrt() * 100.0  // Annualized %
    };
    
    let realized_vol_20d = calc_volatility(i.saturating_sub(vol_period), i);
    let baseline_vol_90d = calc_volatility(i.saturating_sub(baseline_period), i);
    
    let vol_ratio = if baseline_vol_90d > 0.0 { realized_vol_20d / baseline_vol_90d } else { 1.0 };
    let is_vol_compressed = vol_ratio < compression_ratio;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: BOLLINGER BAND WIDTH (squeeze detection)
    // ═══════════════════════════════════════════════════════════════════
    let (bb_upper, bb_middle, bb_lower) = bollinger_bands(candles, bb_period, bb_std);
    
    // Calculate bandwidth history
    let mut bandwidths: Vec<f64> = Vec::new();
    for j in (i.saturating_sub(bw_lookback))..=i {
        if j < bb_upper.len() && bb_middle[j] > 0.0 {
            let bw = (bb_upper[j] - bb_lower[j]) / bb_middle[j];
            bandwidths.push(bw);
        }
    }
    
    let current_bw = if i < bb_upper.len() && bb_middle[i] > 0.0 {
        (bb_upper[i] - bb_lower[i]) / bb_middle[i]
    } else {
        0.0
    };
    
    let min_bw = bandwidths.iter().cloned().fold(f64::INFINITY, f64::min);
    let is_bw_at_low = current_bw <= min_bw * 1.05;  // Within 5% of 6-month low
    
    let squeeze_active = is_vol_compressed && is_bw_at_low;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: DONCHIAN CHANNEL FOR BREAKOUT DETECTION
    // ═══════════════════════════════════════════════════════════════════
    let (dc_upper, dc_lower, _) = donchian(candles, breakout_period);
    
    let dc_high = if i > 0 && i - 1 < dc_upper.len() { dc_upper[i - 1] } else { price }; // Previous day's channel
    let dc_low = if i > 0 && i - 1 < dc_lower.len() { dc_lower[i - 1] } else { price };
    
    let breakout_up = price > dc_high;
    let breakout_down = price < dc_low;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: VOLUME CONFIRMATION
    // ═══════════════════════════════════════════════════════════════════
    // Note: Gold CFDs often don't have reliable volume, so this is optional
    let current_vol = candles[i].volume.unwrap_or(0.0);
    let avg_vol: f64 = if n > vol_period {
        candles[i.saturating_sub(vol_period)..i]
            .iter()
            .filter_map(|c| c.volume)
            .sum::<f64>() / vol_period as f64
    } else {
        current_vol
    };
    
    let volume_ok = avg_vol == 0.0 || current_vol >= avg_vol * vol_confirm;  // Skip check if no volume data
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: ATR FOR STOPS/TARGETS
    // ═══════════════════════════════════════════════════════════════════
    let atr_values = atr(candles, atr_period);
    let atr_val = if i < atr_values.len() { atr_values[i] } else { 0.0 };
    let stop_loss = atr_val * atr_sl_mult;
    let take_profit = atr_val * atr_tp_mult;
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let direction = if breakout_up { "LONG" } else if breakout_down { "SHORT" } else { "NEUTRAL" };
    
    // Build volume string to avoid temporary value borrow error
    let vol_val_str = if avg_vol > 0.0 {
        format!("{:.0}% avg", (current_vol / avg_vol.max(1.0)) * 100.0)
    } else {
        "N/A".to_string()
    };

    let conditions = vec![
        SignalCondition::new("Volatility Compression", is_vol_compressed, &format!("{:.1}%", realized_vol_20d))
            .with_target(&format!("< {:.0}% of baseline ({:.1}%)", compression_ratio * 100.0, baseline_vol_90d * compression_ratio))
            .with_description(&format!("20d vol {:.1}% vs 90d baseline {:.1}%", realized_vol_20d, baseline_vol_90d)),
        
        SignalCondition::new("BB Width Squeeze", is_bw_at_low, &format!("{:.4}", current_bw))
            .with_target(&format!("Near 6-month low ({:.4})", min_bw))
            .with_description(if is_bw_at_low { "Bandwidth at multi-month low - squeeze active" } else { "Bandwidth not at extreme low" }),
        
        SignalCondition::new("Squeeze Status", squeeze_active, if squeeze_active { "ACTIVE" } else { "Waiting" })
            .with_description(if squeeze_active { "Both volatility and price compressed - ready for breakout" } else { "Waiting for full compression" }),
        
        SignalCondition::new("Breakout Signal", breakout_up || breakout_down, 
            if breakout_up { "LONG BREAKOUT" } else if breakout_down { "SHORT BREAKOUT" } else { "None" })
            .with_target(&format!("Close > {:.2} or < {:.2}", dc_high, dc_low))
            .with_description(&format!("Price {:.2}, 20d range: {:.2} - {:.2}", price, dc_low, dc_high)),
        
        SignalCondition::new("Volume Confirm", volume_ok, &vol_val_str)
            .with_target(&format!("> {:.0}% avg", vol_confirm * 100.0))
            .with_description(if volume_ok { "Volume confirms breakout" } else { "Low volume - weak breakout" }),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    let status = format!(
        "Vol: {:.1}% ({:.0}% of baseline) | BW: {:.4} {} | Squeeze: {}",
        realized_vol_20d,
        vol_ratio * 100.0,
        current_bw,
        if is_bw_at_low { "(LOW)" } else { "" },
        if squeeze_active { "✓" } else { "✗" }
    );
    
    // LONG SIGNAL: Squeeze + Breakout up + Volume
    if squeeze_active && breakout_up && volume_ok {
        return SignalResult::buy_with_conditions(
            0.75,
            &format!("🟢 LONG BREAKOUT | {} | SL: ${:.2} TP: ${:.2}", status, stop_loss, take_profit),
            conditions
        );
    }
    
    // SHORT SIGNAL: Squeeze + Breakout down + Volume  
    if squeeze_active && breakout_down && volume_ok {
        return SignalResult::sell_with_conditions(
            0.75,
            &format!("🔴 SHORT BREAKOUT | {} | SL: ${:.2} TP: ${:.2}", status, stop_loss, take_profit),
            conditions
        );
    }
    
    // Squeeze active but no breakout yet
    if squeeze_active {
        return SignalResult::hold_with_conditions(
            &format!("⏳ SQUEEZE ACTIVE | {} | Waiting for breakout", status),
            conditions,
            direction
        );
    }
    
    // No squeeze - waiting for compression
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | Waiting for compression", status),
        conditions,
        direction
    )
}

fn custom_momentum_signal(candles: &[Candle], mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    // Fallback for custom strategies - use EMA + RSI
    let ema_fast = ema(candles, 8);
    let ema_slow = ema(candles, 21);
    let rsi_values = rsi(candles, 14);
    
    let i = candles.len() - 1;
    if i < 1 || i >= ema_fast.len() || i >= rsi_values.len() {
        let conditions = vec![
            SignalCondition::new("EMA(8) Fast", false, "Calculating...")
                .with_description("Short-term trend"),
            SignalCondition::new("EMA(21) Slow", false, "Calculating...")
                .with_description("Medium-term trend"),
            SignalCondition::new("RSI(14)", false, "Calculating...")
                .with_description("Momentum oscillator"),
            SignalCondition::new("Momentum Signal", false, "Pending")
                .with_target("EMA alignment + RSI confirmation"),
        ];
        return SignalResult::hold_with_conditions("Calculating indicators...", conditions, "NEUTRAL");
    }
    
    let ema_f = ema_fast[i];
    let ema_s = ema_slow[i];
    let rsi_val = rsi_values[i];
    let rsi_prev = rsi_values[i - 1];
    
    let is_uptrend = ema_f > ema_s;
    let direction = if is_uptrend { "LONG" } else { "SHORT" };
    let rsi_in_zone = rsi_val > 40.0 && rsi_val < 60.0;
    let rsi_rising = rsi_val > rsi_prev;
    let rsi_falling = rsi_val < rsi_prev;
    
    let bullish_setup = is_uptrend && rsi_in_zone && rsi_rising;
    let bearish_setup = !is_uptrend && rsi_in_zone && rsi_falling;
    
    let conditions = vec![
        SignalCondition::new("EMA(8) Fast", true, &format!("${:.2}", ema_f))
            .with_description("Short-term moving average"),
        SignalCondition::new("EMA(21) Slow", true, &format!("${:.2}", ema_s))
            .with_description("Medium-term moving average"),
        SignalCondition::new("Trend Direction", true, if is_uptrend { "BULLISH ↑" } else { "BEARISH ↓" })
            .with_target(if is_uptrend { "EMA8 > EMA21" } else { "EMA8 < EMA21" })
            .with_description(&format!("Diff: ${:.2}", (ema_f - ema_s).abs())),
        SignalCondition::new("RSI(14)", true, &format!("{:.1}", rsi_val))
            .with_target("40-60 (neutral zone)")
            .with_description(if rsi_rising { "Rising ↑" } else if rsi_falling { "Falling ↓" } else { "Flat" }),
        SignalCondition::new("RSI in Zone", rsi_in_zone,
            if rsi_in_zone { "✓ In neutral zone" } else { "Outside zone" })
            .with_target("40 < RSI < 60")
            .with_description("Avoid overbought/oversold extremes"),
        SignalCondition::new("RSI Momentum", (is_uptrend && rsi_rising) || (!is_uptrend && rsi_falling),
            if rsi_rising { "Rising (bullish)" } else if rsi_falling { "Falling (bearish)" } else { "Flat" })
            .with_target(if is_uptrend { "RSI rising" } else { "RSI falling" })
            .with_description("Momentum confirms trend direction"),
        SignalCondition::new("Bullish Entry", bullish_setup,
            if bullish_setup { "✓ READY" } else { "Waiting..." })
            .with_target("Uptrend + RSI in zone + Rising")
            .with_description("Momentum buy setup"),
        SignalCondition::new("Bearish Entry", bearish_setup,
            if bearish_setup { "✓ READY" } else { "Waiting..." })
            .with_target("Downtrend + RSI in zone + Falling")
            .with_description("Momentum sell setup"),
    ];
    
    // Bullish: EMA trending up, RSI bouncing
    if bullish_setup {
        return SignalResult::buy_with_conditions(0.6, "Momentum bullish setup - EMA up + RSI rising", conditions);
    }
    
    // Bearish: EMA trending down, RSI falling
    if bearish_setup {
        return SignalResult::sell_with_conditions(0.6, "Momentum bearish setup - EMA down + RSI falling", conditions);
    }
    
    SignalResult::hold_with_conditions(
        &format!("EMA: {} | RSI: {:.1} {} | Waiting for setup", 
            if is_uptrend { "Bullish" } else { "Bearish" },
            rsi_val,
            if rsi_rising { "↑" } else if rsi_falling { "↓" } else { "→" }),
        conditions,
        direction
    )
}

/// Gold Trendline Bounce System
/// 
/// Structure-based mean reversion within a trend:
/// 1. Detect swing highs/lows using lookback window
/// 2. Build trendline via linear regression through swings
/// 3. Confirm trend strength (slope + ADX)
/// 4. Wait for pullback to trendline
/// 5. Confirm bounce with candlestick patterns
/// 6. ATR-based risk management
fn trendline_bounce_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let swing_lookback = params.get("swing_lookback").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let min_swing_points = params.get("min_swing_points").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
    let min_slope = params.get("min_slope").and_then(|v| v.as_f64()).unwrap_or(0.05);  // Min slope per candle
    let adx_threshold = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let touch_atr_mult = params.get("touch_atr_mult").and_then(|v| v.as_f64()).unwrap_or(0.5);  // Distance to trendline
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(1.0);  // SL below trendline
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(2.5);  // Take profit
    let ema_trail_period = params.get("ema_trail_period").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    
    let n = candles.len();
    let min_candles = swing_lookback * 2 + 30;  // Need enough for swing detection + indicators
    if n < min_candles {
        let progress = (n as f64 / min_candles as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", n, min_candles))
                .with_target(&format!("{} candles required", min_candles))
                .with_description(&format!("{:.0}% complete — collecting historical data", progress)),
            SignalCondition::new("Swing Point Detection", false, "Pending")
                .with_target(&format!("≥ {} swing points", min_swing_points))
                .with_description("Identifying swing highs and lows for trendline"),
            SignalCondition::new("Trend Strength (ADX)", false, "Pending")
                .with_target(&format!("> {:.0}", adx_threshold))
                .with_description("ADX confirms trending market"),
            SignalCondition::new("Pullback to Trendline", false, "Pending")
                .with_target(&format!("Within {:.1} ATR of trendline", touch_atr_mult))
                .with_description("Price retreats to trendline support/resistance"),
            SignalCondition::new("Bounce Confirmation", false, "Pending")
                .with_target("Rejection candle pattern")
                .with_description("Candlestick confirms bounce off trendline"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Initializing: {}/{} candles ({:.0}%)", n, min_candles, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let i = n - 1;
    let price = candles[i].close;
    let current_high = candles[i].high;
    let current_low = candles[i].low;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: DETECT SWING POINTS
    // ═══════════════════════════════════════════════════════════════════
    // Swing low: lowest low of lookback candles on both sides
    // Swing high: highest high of lookback candles on both sides
    
    let mut swing_lows: Vec<(usize, f64)> = Vec::new();  // (index, price)
    let mut swing_highs: Vec<(usize, f64)> = Vec::new();
    
    for j in swing_lookback..(n - swing_lookback) {
        let low = candles[j].low;
        let high = candles[j].high;
        
        // Check if this is a swing low
        let is_swing_low = (j.saturating_sub(swing_lookback)..j)
            .all(|k| candles[k].low >= low)
            && ((j + 1)..=(j + swing_lookback).min(n - 1))
            .all(|k| candles[k].low >= low);
        
        // Check if this is a swing high
        let is_swing_high = (j.saturating_sub(swing_lookback)..j)
            .all(|k| candles[k].high <= high)
            && ((j + 1)..=(j + swing_lookback).min(n - 1))
            .all(|k| candles[k].high <= high);
        
        if is_swing_low {
            swing_lows.push((j, low));
        }
        if is_swing_high {
            swing_highs.push((j, high));
        }
    }
    
    // Keep only recent swings (last 50 candles worth)
    let recent_cutoff = i.saturating_sub(50);
    swing_lows.retain(|(idx, _)| *idx >= recent_cutoff);
    swing_highs.retain(|(idx, _)| *idx >= recent_cutoff);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: BUILD TRENDLINES VIA LINEAR REGRESSION
    // ═══════════════════════════════════════════════════════════════════
    // Linear regression: y = slope * x + intercept
    
    fn linear_regression(points: &[(usize, f64)]) -> Option<(f64, f64)> {
        if points.len() < 2 {
            return None;
        }
        let n = points.len() as f64;
        let sum_x: f64 = points.iter().map(|(x, _)| *x as f64).sum();
        let sum_y: f64 = points.iter().map(|(_, y)| *y).sum();
        let sum_xy: f64 = points.iter().map(|(x, y)| *x as f64 * y).sum();
        let sum_xx: f64 = points.iter().map(|(x, _)| (*x as f64).powi(2)).sum();
        
        let denom = n * sum_xx - sum_x.powi(2);
        if denom.abs() < 1e-10 {
            return None;
        }
        
        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / n;
        
        Some((slope, intercept))
    }
    
    // Get last N swing points for trendline
    let recent_lows: Vec<(usize, f64)> = swing_lows.iter().rev().take(min_swing_points + 1).cloned().collect();
    let recent_highs: Vec<(usize, f64)> = swing_highs.iter().rev().take(min_swing_points + 1).cloned().collect();
    
    let uptrend_line = linear_regression(&recent_lows);
    let downtrend_line = linear_regression(&recent_highs);
    
    // Calculate trendline price at current candle
    let uptrend_price = uptrend_line.map(|(slope, intercept)| slope * i as f64 + intercept);
    let downtrend_price = downtrend_line.map(|(slope, intercept)| slope * i as f64 + intercept);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: CONFIRM TREND STRENGTH
    // ═══════════════════════════════════════════════════════════════════
    let (adx_line, _, _) = adx(candles, 14);  // ADX returns (adx, +di, -di)
    let adx_val = if i < adx_line.len() { adx_line[i] } else { 0.0 };
    let adx_ok = adx_val >= adx_threshold;
    
    // Check slopes
    let uptrend_slope = uptrend_line.map(|(s, _)| s).unwrap_or(0.0);
    let downtrend_slope = downtrend_line.map(|(s, _)| s).unwrap_or(0.0);
    
    let is_uptrend = uptrend_slope > min_slope && uptrend_price.is_some();
    let is_downtrend = downtrend_slope < -min_slope && downtrend_price.is_some();
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: DETECT PULLBACK TO TRENDLINE
    // ═══════════════════════════════════════════════════════════════════
    let atr_values = atr(candles, atr_period);
    let atr_val = if i < atr_values.len() { atr_values[i] } else { 0.0 };
    let touch_distance = atr_val * touch_atr_mult;
    
    // Check if price is touching uptrend support
    let touching_uptrend = uptrend_price
        .map(|tl| current_low <= tl + touch_distance && current_low >= tl - touch_distance)
        .unwrap_or(false);
    
    // Check if price is touching downtrend resistance  
    let touching_downtrend = downtrend_price
        .map(|tl| current_high >= tl - touch_distance && current_high <= tl + touch_distance)
        .unwrap_or(false);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: BOUNCE CONFIRMATION (Candlestick Patterns)
    // ═══════════════════════════════════════════════════════════════════
    let prev = &candles[i - 1];
    let curr = &candles[i];
    
    let body_curr = (curr.close - curr.open).abs();
    let body_prev = (prev.close - prev.open).abs();
    let range_curr = curr.high - curr.low;
    
    // Bullish patterns (for uptrend bounce)
    let bullish_engulfing = curr.close > curr.open  // Green candle
        && prev.close < prev.open  // Previous was red
        && curr.open <= prev.close  // Opens at or below prev close
        && curr.close >= prev.open;  // Closes at or above prev open
    
    let hammer = curr.close > curr.open  // Green candle
        && body_curr > 0.0
        && (curr.open - curr.low) >= body_curr * 2.0  // Long lower wick
        && (curr.high - curr.close) <= body_curr * 0.5;  // Small upper wick
    
    let bullish_bounce = (bullish_engulfing || hammer) && curr.close > prev.high;
    
    // Bearish patterns (for downtrend bounce)
    let bearish_engulfing = curr.close < curr.open  // Red candle
        && prev.close > prev.open  // Previous was green
        && curr.open >= prev.close  // Opens at or above prev close
        && curr.close <= prev.open;  // Closes at or below prev open
    
    let shooting_star = curr.close < curr.open  // Red candle
        && body_curr > 0.0
        && (curr.high - curr.open) >= body_curr * 2.0  // Long upper wick
        && (curr.close - curr.low) <= body_curr * 0.5;  // Small lower wick
    
    let bearish_bounce = (bearish_engulfing || shooting_star) && curr.close < prev.low;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 6: CALCULATE RISK PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let stop_loss = atr_val * atr_sl_mult;
    let take_profit = atr_val * atr_tp_mult;
    
    // Previous swing for TP target
    let prev_swing_high = swing_highs.last().map(|(_, p)| *p).unwrap_or(price + take_profit);
    let prev_swing_low = swing_lows.last().map(|(_, p)| *p).unwrap_or(price - take_profit);
    
    // Trailing EMA
    let ema_trail = ema(candles, ema_trail_period);
    let ema_val = if i < ema_trail.len() { ema_trail[i] } else { price };
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let direction = if is_uptrend { "LONG" } else if is_downtrend { "SHORT" } else { "NEUTRAL" };
    
    let trend_desc = if is_uptrend {
        format!("Uptrend: {:.2} pips/candle, {} swing lows", uptrend_slope, recent_lows.len())
    } else if is_downtrend {
        format!("Downtrend: {:.2} pips/candle, {} swing highs", downtrend_slope.abs(), recent_highs.len())
    } else {
        "No clear trend structure".to_string()
    };
    
    let tl_price_str = if is_uptrend {
        uptrend_price.map(|p| format!("${:.2}", p)).unwrap_or("N/A".to_string())
    } else {
        downtrend_price.map(|p| format!("${:.2}", p)).unwrap_or("N/A".to_string())
    };
    
    let touch_status = if is_uptrend && touching_uptrend {
        "Touching support"
    } else if is_downtrend && touching_downtrend {
        "Touching resistance"
    } else {
        "Away from trendline"
    };
    
    let bounce_pattern = if bullish_bounce {
        if bullish_engulfing { "Bullish Engulfing" } else { "Hammer" }
    } else if bearish_bounce {
        if bearish_engulfing { "Bearish Engulfing" } else { "Shooting Star" }
    } else {
        "None"
    };
    
    let conditions = vec![
        SignalCondition::new("Swing Points", swing_lows.len() >= min_swing_points || swing_highs.len() >= min_swing_points,
            &format!("{} lows, {} highs", swing_lows.len(), swing_highs.len()))
            .with_target(&format!(">= {} points", min_swing_points))
            .with_description("Objective market structure identified"),
        
        SignalCondition::new("Trendline", is_uptrend || is_downtrend, &tl_price_str)
            .with_target(&format!("Slope > {:.2}", min_slope))
            .with_description(&trend_desc),
        
        SignalCondition::new("ADX Trend Strength", adx_ok, &format!("{:.1}", adx_val))
            .with_target(&format!("> {:.0}", adx_threshold))
            .with_description(if adx_ok { "Strong trend confirmed" } else { "Weak/ranging market" }),
        
        SignalCondition::new("Pullback to Trendline", 
            (is_uptrend && touching_uptrend) || (is_downtrend && touching_downtrend),
            touch_status)
            .with_target(&format!("Within {:.1} ATR of line", touch_atr_mult))
            .with_description(&format!("Distance threshold: ${:.2}", touch_distance)),
        
        SignalCondition::new("Bounce Confirmation",
            (is_uptrend && bullish_bounce) || (is_downtrend && bearish_bounce),
            bounce_pattern)
            .with_target(if is_uptrend { "Bullish candle pattern" } else { "Bearish candle pattern" })
            .with_description("Rejection candle confirms bounce"),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    let status = format!(
        "Trend: {} | ADX: {:.0} {} | TL: {} | {}",
        if is_uptrend { "UP" } else if is_downtrend { "DOWN" } else { "NONE" },
        adx_val,
        if adx_ok { "✓" } else { "(weak)" },
        tl_price_str,
        touch_status
    );
    
    // LONG SIGNAL: Uptrend + Touching support + Bullish bounce + ADX strong
    if is_uptrend && adx_ok && touching_uptrend && bullish_bounce {
        let tl = uptrend_price.unwrap_or(price);
        let sl_price = tl - stop_loss;
        let tp_price = prev_swing_high.min(price + take_profit);
        let confidence: f64 = 0.70 + ((adx_val - adx_threshold) / 40.0).min(0.15);
        
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!("🟢 LONG BOUNCE | {} | {} | SL: ${:.2} TP: ${:.2}", 
                status, bounce_pattern, sl_price, tp_price),
            conditions
        );
    }
    
    // SHORT SIGNAL: Downtrend + Touching resistance + Bearish bounce + ADX strong
    if is_downtrend && adx_ok && touching_downtrend && bearish_bounce {
        let tl = downtrend_price.unwrap_or(price);
        let sl_price = tl + stop_loss;
        let tp_price = prev_swing_low.max(price - take_profit);
        let confidence: f64 = 0.70 + ((adx_val - adx_threshold) / 40.0).min(0.15);
        
        return SignalResult::sell_with_conditions(
            confidence.min(0.85),
            &format!("🔴 SHORT BOUNCE | {} | {} | SL: ${:.2} TP: ${:.2}", 
                status, bounce_pattern, sl_price, tp_price),
            conditions
        );
    }
    
    // Waiting for pullback
    if (is_uptrend || is_downtrend) && adx_ok {
        return SignalResult::hold_with_conditions(
            &format!("⏳ {} | Waiting for pullback to trendline", status),
            conditions,
            direction
        );
    }
    
    // No valid trend structure
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | Building structure", status),
        conditions,
        direction
    )
}

/// BTC Liquidity Sweep Reversal System
/// 
/// Trades fake breakouts (stop hunts) at swing highs/lows:
/// 1. Detect swing highs/lows (20 candle lookback)
/// 2. Wait for price to break level then close back (within 3 candles)
/// 3. RSI exhaustion confirmation (>70 for shorts, <30 for longs)
/// 4. EMA200 trend filter (optional)
/// 5. Aggressive 4:1 R:R targeting
fn liquidity_sweep_signal(candles: &[Candle], params: &serde_json::Value, mrate_weight: Option<f64>) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS
    // ═══════════════════════════════════════════════════════════════════
    let swing_lookback = params.get("swing_lookback").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let sweep_window = params.get("sweep_window").and_then(|v| v.as_u64()).unwrap_or(3) as usize;  // Candles to close back
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let rsi_overbought = params.get("rsi_overbought").and_then(|v| v.as_f64()).unwrap_or(70.0);
    let rsi_oversold = params.get("rsi_oversold").and_then(|v| v.as_f64()).unwrap_or(30.0);
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(1.0);  // SL beyond sweep wick
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(4.0);  // 4:1 R:R
    let use_trend_filter = params.get("use_trend_filter").and_then(|v| v.as_bool()).unwrap_or(true);
    let trend_ema = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    
    let n = candles.len();
    let min_candles = swing_lookback + sweep_window + 10;
    if n < min_candles {
        let progress = (n as f64 / min_candles as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", n, min_candles))
                .with_target(&format!("{} candles required", min_candles))
                .with_description(&format!("{:.0}% complete — collecting historical data", progress)),
            SignalCondition::new("Swing Level Detection", false, "Pending")
                .with_target(&format!("{}-candle swing high/low", swing_lookback))
                .with_description("Identifying liquidity levels at swing extremes"),
            SignalCondition::new("Liquidity Sweep", false, "Pending")
                .with_target(&format!("Break + close back within {} candles", sweep_window))
                .with_description("Detecting fake breakouts / stop hunts"),
            SignalCondition::new("RSI Exhaustion", false, "Pending")
                .with_target(&format!("< {:.0} or > {:.0}", rsi_oversold, rsi_overbought))
                .with_description("RSI confirms exhaustion at sweep level"),
            SignalCondition::new("Trend Filter (EMA)", false, "Pending")
                .with_target(&format!("EMA {} alignment", trend_ema))
                .with_description(if use_trend_filter { "Only trade with trend" } else { "Disabled" }),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Initializing: {}/{} candles ({:.0}%)", n, min_candles, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let i = n - 1;
    let price = candles[i].close;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: DETECT SWING HIGH/LOW (Liquidity Levels)
    // ═══════════════════════════════════════════════════════════════════
    // Look back to find the swing high/low BEFORE the sweep window
    let lookback_end = i.saturating_sub(sweep_window);
    let lookback_start = lookback_end.saturating_sub(swing_lookback);
    
    let swing_high = candles[lookback_start..lookback_end]
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    
    let swing_low = candles[lookback_start..lookback_end]
        .iter()
        .map(|c| c.low)
        .fold(f64::INFINITY, f64::min);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: DETECT LIQUIDITY SWEEP (Break + Close Back)
    // ═══════════════════════════════════════════════════════════════════
    // Check recent candles for sweep pattern
    let recent_candles = &candles[lookback_end..=i];
    
    // Bull trap (short setup): Broke above swing high, now closed back below
    let broke_high = recent_candles.iter().any(|c| c.high > swing_high);
    let sweep_wick_high = recent_candles.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
    let closed_back_below = price < swing_high;
    let bull_trap = broke_high && closed_back_below;
    
    // Bear trap (long setup): Broke below swing low, now closed back above
    let broke_low = recent_candles.iter().any(|c| c.low < swing_low);
    let sweep_wick_low = recent_candles.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let closed_back_above = price > swing_low;
    let bear_trap = broke_low && closed_back_above;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: RSI EXHAUSTION
    // ═══════════════════════════════════════════════════════════════════
    let rsi_values = rsi(candles, rsi_period);
    let rsi_curr = if i < rsi_values.len() { rsi_values[i] } else { 50.0 };
    // Check if RSI was overbought/oversold during the sweep
    let rsi_in_sweep = &rsi_values[lookback_end.min(rsi_values.len().saturating_sub(1))..i.min(rsi_values.len())];
    let max_rsi_in_sweep = rsi_in_sweep.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_rsi_in_sweep = rsi_in_sweep.iter().cloned().fold(f64::INFINITY, f64::min);
    
    let rsi_overbought_hit = max_rsi_in_sweep >= rsi_overbought;
    let rsi_oversold_hit = min_rsi_in_sweep <= rsi_oversold;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: TREND FILTER (EMA200)
    // ═══════════════════════════════════════════════════════════════════
    let ema_values = ema(candles, trend_ema);
    let ema_val = if i < ema_values.len() { ema_values[i] } else { price };
    let is_uptrend = price > ema_val;
    let is_downtrend = price < ema_val;
    
    // If trend filter enabled: only long in uptrend, short in downtrend
    let trend_ok_for_long = !use_trend_filter || is_uptrend;
    let trend_ok_for_short = !use_trend_filter || is_downtrend;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: ATR FOR STOPS/TARGETS
    // ═══════════════════════════════════════════════════════════════════
    let atr_values = atr(candles, atr_period);
    let atr_val = if i < atr_values.len() { atr_values[i] } else { 0.0 };
    
    // ═══════════════════════════════════════════════════════════════════
    // BUILD STRUCTURED CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let direction = if bear_trap && rsi_oversold_hit { "LONG" } 
        else if bull_trap && rsi_overbought_hit { "SHORT" } 
        else { "NEUTRAL" };
    
    let sweep_status = if bull_trap {
        format!("BULL TRAP: Swept ${:.2}, closed back at ${:.2}", sweep_wick_high, price)
    } else if bear_trap {
        format!("BEAR TRAP: Swept ${:.2}, closed back at ${:.2}", sweep_wick_low, price)
    } else {
        "No sweep detected".to_string()
    };
    
    let trend_status = if use_trend_filter {
        format!("EMA{}: ${:.2} ({})", trend_ema, ema_val, if is_uptrend { "ABOVE" } else { "BELOW" })
    } else {
        "Disabled".to_string()
    };
    
    // Pre-format descriptions to avoid borrow issues
    let bull_trap_desc = if bull_trap { 
        format!("Swept ${:.2}", sweep_wick_high) 
    } else { 
        "Waiting for sweep above swing high".to_string() 
    };
    let bear_trap_desc = if bear_trap { 
        format!("Swept ${:.2}", sweep_wick_low) 
    } else { 
        "Waiting for sweep below swing low".to_string() 
    };
    
    let conditions = vec![
        SignalCondition::new("Swing High", true, &format!("${:.2}", swing_high))
            .with_description(&format!("Last {} candle high", swing_lookback)),
        
        SignalCondition::new("Swing Low", true, &format!("${:.2}", swing_low))
            .with_description(&format!("Last {} candle low", swing_lookback)),
        
        SignalCondition::new("Bull Trap (Short)", bull_trap && rsi_overbought_hit, 
            if bull_trap { "DETECTED" } else { "None" })
            .with_target("Break high + close back below")
            .with_description(&bull_trap_desc),
        
        SignalCondition::new("Bear Trap (Long)", bear_trap && rsi_oversold_hit,
            if bear_trap { "DETECTED" } else { "None" })
            .with_target("Break low + close back above")
            .with_description(&bear_trap_desc),
        
        SignalCondition::new("RSI Exhaustion", 
            (bull_trap && rsi_overbought_hit) || (bear_trap && rsi_oversold_hit),
            &format!("{:.1} (max: {:.1}, min: {:.1})", rsi_curr, max_rsi_in_sweep, min_rsi_in_sweep))
            .with_target(&format!(">{:.0} for short, <{:.0} for long", rsi_overbought, rsi_oversold))
            .with_description("RSI must hit extreme during sweep"),
        
        SignalCondition::new("Trend Filter", 
            (bear_trap && trend_ok_for_long) || (bull_trap && trend_ok_for_short) || !use_trend_filter,
            &trend_status)
            .with_target("Long in uptrend, Short in downtrend")
            .with_description(if use_trend_filter { "Filters against-trend setups" } else { "Disabled - trading both directions" }),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // CHECK FOR SIGNALS
    // ═══════════════════════════════════════════════════════════════════
    let status = format!(
        "High: ${:.2} | Low: ${:.2} | RSI: {:.1} | {}",
        swing_high, swing_low, rsi_curr,
        if is_uptrend { "UPTREND" } else { "DOWNTREND" }
    );
    
    // LONG SIGNAL: Bear trap + RSI oversold + (trend filter ok)
    if bear_trap && rsi_oversold_hit && trend_ok_for_long {
        let sl_price = sweep_wick_low - (atr_val * atr_sl_mult);
        let tp_price = price + (atr_val * atr_tp_mult);
        let risk = price - sl_price;
        let reward = tp_price - price;
        let confidence: f64 = 0.65 + (if is_uptrend { 0.1 } else { 0.0 });
        
        return SignalResult::buy_with_conditions(
            confidence.min(0.80),
            &format!("🟢 LONG REVERSAL | Bear trap at ${:.2} | R:R {:.1}:1 | SL: ${:.2} TP: ${:.2}", 
                sweep_wick_low, reward / risk, sl_price, tp_price),
            conditions
        );
    }
    
    // SHORT SIGNAL: Bull trap + RSI overbought + (trend filter ok)
    if bull_trap && rsi_overbought_hit && trend_ok_for_short {
        let sl_price = sweep_wick_high + (atr_val * atr_sl_mult);
        let tp_price = price - (atr_val * atr_tp_mult);
        let risk = sl_price - price;
        let reward = price - tp_price;
        let confidence: f64 = 0.65 + (if is_downtrend { 0.1 } else { 0.0 });
        
        return SignalResult::sell_with_conditions(
            confidence.min(0.80),
            &format!("🔴 SHORT REVERSAL | Bull trap at ${:.2} | R:R {:.1}:1 | SL: ${:.2} TP: ${:.2}", 
                sweep_wick_high, reward / risk, sl_price, tp_price),
            conditions
        );
    }
    
    // Partial setup - sweep detected but missing RSI
    if bull_trap || bear_trap {
        return SignalResult::hold_with_conditions(
            &format!("⏳ {} | Waiting for RSI exhaustion", sweep_status),
            conditions,
            direction
        );
    }
    
    // No sweep - watching levels
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | Watching for sweep", status),
        conditions,
        direction
    )
}

/// Macro-Aligned Momentum Strategy (MAM)
/// 
/// Combines Polymarket macro sentiment with technical momentum signals.
/// Only trades when macro regime aligns with technical direction.
/// Position sizing scales with macro conviction.
/// 
/// Formula: macro_score = ΔRATE_CUT + ΔINFLATION - (ΔRECESSION × 0.5)
/// Regime: RISK_ON (>15), RISK_OFF (<-15), NEUTRAL (between)
pub fn macro_aligned_momentum_signal(
    candles: &[Candle],
    params: &serde_json::Value,
    mrate_weight: Option<f64>,
) -> SignalResult {
    let _relaxation = calculate_mrate_relaxation(mrate_weight);
    let n = candles.len();
    if n < 201 {
        let progress = (n as f64 / 201.0 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/201 candles", n))
                .with_target("201 candles required")
                .with_description(&format!("{:.0}% complete — collecting historical data", progress)),
            SignalCondition::new("Macro Regime", false, "Pending")
                .with_target("RISK_ON / RISK_OFF")
                .with_description("Polymarket macro sentiment score"),
            SignalCondition::new("Trend (EMA200)", false, "Pending")
                .with_target("Directional bias")
                .with_description("Waiting for sufficient data"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Collecting data: {}/201 candles ({:.0}%)", n, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    // === PARAMETERS ===
    let trend_ema_period = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
    let value_ema_period = params.get("value_ema").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let rsi_oversold = params.get("rsi_oversold").and_then(|v| v.as_f64()).unwrap_or(35.0);
    let rsi_overbought = params.get("rsi_overbought").and_then(|v| v.as_f64()).unwrap_or(65.0);
    let adx_threshold = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(1.5);
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(3.0);
    let breakout_period = params.get("breakout_period").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    
    // Macro regime parameters (passed from runner after fetching from DB)
    let macro_score = params.get("macro_score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let macro_regime = params.get("macro_regime").and_then(|v| v.as_str()).unwrap_or("NEUTRAL");
    let position_multiplier = params.get("position_multiplier").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let breakout_mode = params.get("breakout_mode").and_then(|v| v.as_bool()).unwrap_or(false);
    let symbol = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("XAU_USD");
    let is_btc = symbol.contains("BTC");
    
    // === TECHNICAL INDICATORS ===
    let i = n - 1;
    let price = candles[i].close;
    
    // Trend EMA (200)
    let trend_ema = ema(candles, trend_ema_period);
    if i >= trend_ema.len() {
        let conditions = vec![
            SignalCondition::new("Trend EMA", false, "Calculating...")
                .with_target(&format!("EMA({}) required", trend_ema_period))
                .with_description("Need more candles to compute trend"),
            SignalCondition::new("Macro Regime", false, "Pending")
                .with_description("Waiting for EMA calculation"),
        ];
        return SignalResult::hold_with_conditions("Calculating trend EMA...", conditions, "NEUTRAL");
    }
    let trend_ema_val = trend_ema[i];
    let is_uptrend = price > trend_ema_val;
    let is_downtrend = price < trend_ema_val;
    
    // Value EMA (21) - pullback zone
    let value_ema = ema(candles, value_ema_period);
    let value_ema_val = if i < value_ema.len() { value_ema[i] } else { price };
    let in_value_zone = (price - value_ema_val).abs() / value_ema_val < 0.01; // Within 1%
    
    // RSI
    let rsi_vals = rsi(candles, rsi_period);
    let rsi_val = if i < rsi_vals.len() { rsi_vals[i] } else { 50.0 };
    let rsi_prev = if i > 0 && i - 1 < rsi_vals.len() { rsi_vals[i - 1] } else { 50.0 };
    let rsi_oversold_hit = rsi_val < rsi_oversold;
    let rsi_overbought_hit = rsi_val > rsi_overbought;
    let rsi_turning_up = rsi_val > rsi_prev && rsi_prev < rsi_oversold + 10.0;
    let rsi_turning_down = rsi_val < rsi_prev && rsi_prev > rsi_overbought - 10.0;
    
    // ADX - trend strength
    let (adx_vals, _plus_di, _minus_di) = adx(candles, 14);
    let adx_val = if i < adx_vals.len() { adx_vals[i] } else { 20.0 };
    let adx_ok = adx_val >= adx_threshold;
    
    // ATR - for stops
    let atr_vals = atr(candles, atr_period);
    let atr_val = if i < atr_vals.len() { atr_vals[i] } else { 0.0 };
    
    // Donchian channels for breakout mode
    let (dc_upper, dc_lower, _) = donchian(candles, breakout_period);
    let dc_high = if i < dc_upper.len() { dc_upper[i] } else { price };
    let dc_low = if i < dc_lower.len() { dc_lower[i] } else { price };
    let breakout_long = price >= dc_high;
    let breakout_short = price <= dc_low;
    
    // === BUILD CONDITIONS ===
    let mut conditions = Vec::new();
    
    // 1. Macro Regime
    let regime_label = match macro_regime {
        "RISK_ON" => "🟢 RISK ON (Bullish Macro)",
        "RISK_OFF" => "🔴 RISK OFF (Bearish Macro)",
        _ => "⚪ NEUTRAL (Mixed Signals)",
    };
    let regime_ok_for_long = macro_regime == "RISK_ON" || (macro_regime == "NEUTRAL" && !is_btc);
    let regime_ok_for_short = macro_regime == "RISK_OFF" || (macro_regime == "NEUTRAL" && is_btc);
    
    conditions.push(
        SignalCondition::new(
            "Macro Regime",
            macro_regime != "NEUTRAL",
            &format!("{} | Score: {:.1}", regime_label, macro_score)
        )
        .with_target("RISK_ON for longs, RISK_OFF for shorts")
        .with_description(&format!("Position multiplier: {:.1}x", position_multiplier))
    );
    
    // 2. Breakout Mode
    conditions.push(
        SignalCondition::new(
            "Breakout Mode",
            breakout_mode,
            if breakout_mode { "ACTIVE (regime flipped recently)" } else { "Inactive" }
        )
        .with_description(if breakout_mode { 
            "Regime flipped in last 10 days - watching for breakout entries" 
        } else { 
            "Normal mode - using pullback entries" 
        })
    );
    
    // 3. Trend Direction (EMA200)
    let trend_direction = if is_uptrend { "BULLISH" } else { "BEARISH" };
    conditions.push(
        SignalCondition::new(
            &format!("Trend (EMA{})", trend_ema_period),
            true, // Always show current trend
            &format!("{} | Price ${:.2} vs EMA ${:.2}", trend_direction, price, trend_ema_val)
        )
    );
    
    // 4. ADX Trend Strength
    conditions.push(
        SignalCondition::new(
            "ADX Trend Strength",
            adx_ok,
            &format!("{:.1}", adx_val)
        )
        .with_target(&format!("≥ {:.0}", adx_threshold))
        .with_description(if adx_ok { "Trending market" } else { "Weak/ranging - avoid trading" })
    );
    
    // 5. Value Zone (EMA21)
    conditions.push(
        SignalCondition::new(
            &format!("Value Zone (EMA{})", value_ema_period),
            in_value_zone,
            &format!("${:.2} from zone", (price - value_ema_val).abs())
        )
        .with_target("Within 1% of EMA")
        .with_description(if in_value_zone { "In pullback zone" } else { "Extended from mean" })
    );
    
    // 6. RSI Conditions
    let rsi_condition_label = if is_uptrend { "RSI Oversold" } else { "RSI Overbought" };
    let rsi_condition_met = if is_uptrend { rsi_oversold_hit || rsi_turning_up } else { rsi_overbought_hit || rsi_turning_down };
    let rsi_target = if is_uptrend { format!("< {:.0}", rsi_oversold) } else { format!(">{:.0}", rsi_overbought) };
    conditions.push(
        SignalCondition::new(
            rsi_condition_label,
            rsi_condition_met,
            &format!("{:.1} (prev: {:.1})", rsi_val, rsi_prev)
        )
        .with_target(&rsi_target)
        .with_description(if rsi_turning_up { "RSI turning up ↗" } else if rsi_turning_down { "RSI turning down ↘" } else { "Waiting for momentum shift" })
    );
    
    // 7. Breakout (if in breakout mode)
    if breakout_mode {
        let breakout_direction = if macro_regime == "RISK_ON" { "Long" } else { "Short" };
        let breakout_met = if macro_regime == "RISK_ON" { breakout_long } else { breakout_short };
        let breakout_level = if macro_regime == "RISK_ON" { dc_high } else { dc_low };
        conditions.push(
            SignalCondition::new(
                &format!("{}-Day {} Breakout", breakout_period, breakout_direction),
                breakout_met,
                &format!("Price ${:.2} vs Level ${:.2}", price, breakout_level)
            )
            .with_description(if breakout_met { "BREAKOUT CONFIRMED! 🚀" } else { "Waiting for breakout..." })
        );
    }
    
    // === DETERMINE DIRECTION ===
    let direction = if macro_regime == "RISK_ON" || (macro_regime == "NEUTRAL" && is_uptrend) {
        "LONG"
    } else {
        "SHORT"
    };
    
    // === SIGNAL LOGIC ===
    
    // BREAKOUT MODE: Trade breakouts when regime flipped
    if breakout_mode {
        // Long breakout in RISK_ON
        if macro_regime == "RISK_ON" && breakout_long && adx_ok {
            let sl_price = price - (atr_val * atr_sl_mult);
            let tp_price = price + (atr_val * atr_tp_mult);
            let confidence = 0.70 + (macro_score.abs() / 100.0 * 0.15); // Scale with conviction
            
            return SignalResult::buy_with_conditions(
                confidence.min(0.85),
                &format!("🚀 MACRO BREAKOUT LONG | Score: {:.1} | Mult: {:.1}x | SL: ${:.2} TP: ${:.2}",
                    macro_score, position_multiplier, sl_price, tp_price),
                conditions
            );
        }
        
        // Short breakout in RISK_OFF
        if macro_regime == "RISK_OFF" && breakout_short && adx_ok {
            let sl_price = price + (atr_val * atr_sl_mult);
            let tp_price = price - (atr_val * atr_tp_mult);
            let confidence = 0.70 + (macro_score.abs() / 100.0 * 0.15);
            
            return SignalResult::sell_with_conditions(
                confidence.min(0.85),
                &format!("🚀 MACRO BREAKOUT SHORT | Score: {:.1} | Mult: {:.1}x | SL: ${:.2} TP: ${:.2}",
                    macro_score, position_multiplier, sl_price, tp_price),
                conditions
            );
        }
    }
    
    // NORMAL MODE: Pullback entries aligned with macro
    
    // LONG SIGNAL: RISK_ON macro + uptrend + pullback + RSI turning
    if regime_ok_for_long && is_uptrend && in_value_zone && rsi_turning_up && adx_ok {
        let sl_price = price - (atr_val * atr_sl_mult);
        let tp_price = price + (atr_val * atr_tp_mult);
        let base_confidence = 0.65;
        let macro_bonus = if macro_regime == "RISK_ON" { macro_score.abs() / 100.0 * 0.20 } else { 0.0 };
        let confidence = base_confidence + macro_bonus;
        
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!("🟢 MACRO-ALIGNED LONG | Regime: {} | Score: {:.1} | Mult: {:.1}x | SL: ${:.2} TP: ${:.2}",
                macro_regime, macro_score, position_multiplier, sl_price, tp_price),
            conditions
        );
    }
    
    // SHORT SIGNAL: RISK_OFF macro + downtrend + pullback + RSI turning
    if regime_ok_for_short && is_downtrend && in_value_zone && rsi_turning_down && adx_ok {
        let sl_price = price + (atr_val * atr_sl_mult);
        let tp_price = price - (atr_val * atr_tp_mult);
        let base_confidence = 0.65;
        let macro_bonus = if macro_regime == "RISK_OFF" { macro_score.abs() / 100.0 * 0.20 } else { 0.0 };
        let confidence = base_confidence + macro_bonus;
        
        return SignalResult::sell_with_conditions(
            confidence.min(0.85),
            &format!("🔴 MACRO-ALIGNED SHORT | Regime: {} | Score: {:.1} | Mult: {:.1}x | SL: ${:.2} TP: ${:.2}",
                macro_regime, macro_score, position_multiplier, sl_price, tp_price),
            conditions
        );
    }
    
    // NEUTRAL REGIME: Reduce confidence significantly
    if macro_regime == "NEUTRAL" {
        // Still allow signals but with reduced confidence
        if is_uptrend && in_value_zone && rsi_turning_up && adx_ok {
            let sl_price = price - (atr_val * atr_sl_mult);
            let tp_price = price + (atr_val * atr_tp_mult);
            
            return SignalResult::buy_with_conditions(
                0.50, // Reduced confidence in neutral
                &format!("⚪ NEUTRAL LONG | Macro mixed - reduced confidence | SL: ${:.2} TP: ${:.2}",
                    sl_price, tp_price),
                conditions
            );
        }
        
        if is_downtrend && in_value_zone && rsi_turning_down && adx_ok {
            let sl_price = price + (atr_val * atr_sl_mult);
            let tp_price = price - (atr_val * atr_tp_mult);
            
            return SignalResult::sell_with_conditions(
                0.50, // Reduced confidence in neutral
                &format!("⚪ NEUTRAL SHORT | Macro mixed - reduced confidence | SL: ${:.2} TP: ${:.2}",
                    sl_price, tp_price),
                conditions
            );
        }
    }
    
    // No signal - return hold with current state
    let status = format!(
        "Macro: {} ({:.1}) | Trend: {} | RSI: {:.1} | ADX: {:.1}",
        macro_regime, macro_score,
        if is_uptrend { "UP" } else { "DOWN" },
        rsi_val, adx_val
    );
    
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | Waiting for alignment", status),
        conditions,
        direction
    )
}

/// MRATE Regime Trader - Aggressive MRATE-driven strategy
/// 
/// Uses MRATE data as PRIMARY signal source, not just a filter:
/// 1. MRATE regime determines directional bias
/// 2. Favored instrument confirms we're trading the right asset
/// 3. Simple technicals for entry timing (looser than other strategies)
/// 4. More trades when MRATE is clear, fewer when choppy
fn mrate_regime_trader_signal(
    candles: &[Candle],
    params: &serde_json::Value,
    mrate: Option<&MrateOutput>,
) -> SignalResult {
    // Get MRATE data - this strategy REQUIRES it
    let mrate = match mrate {
        Some(m) => m,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", false, "Unavailable")
                    .with_target("MRATE engine output required")
                    .with_description("This strategy requires MRATE regime data to function"),
                SignalCondition::new("Regime Bias", false, "Pending")
                    .with_target("MRATE regime determines direction")
                    .with_description("Waiting for MRATE data"),
                SignalCondition::new("Technical Entry", false, "Pending")
                    .with_target("EMA + RSI alignment")
                    .with_description("Waiting for MRATE data"),
            ];
            return SignalResult::hold_with_conditions(
                "⚠️ MRATE data unavailable — this strategy requires MRATE",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    // ═══════════════════════════════════════════════════════════════════
    // CONFIGURABLE PARAMETERS (TIGHTENED to reduce whipsaw)
    // ═══════════════════════════════════════════════════════════════════
    let trend_ema = params.get("trend_ema").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let entry_ema = params.get("entry_ema").and_then(|v| v.as_u64()).unwrap_or(21) as usize;
    let rsi_period = params.get("rsi_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    // TIGHTENED: RSI zones now require more extreme values (was 45/55)
    let rsi_long_zone = params.get("rsi_long_zone").and_then(|v| v.as_f64()).unwrap_or(40.0);
    let rsi_short_zone = params.get("rsi_short_zone").and_then(|v| v.as_f64()).unwrap_or(60.0);
    let atr_period = params.get("atr_period").and_then(|v| v.as_u64()).unwrap_or(14) as usize;
    // TIGHTENED: Wider stops to avoid getting stopped out on noise (was 1.5)
    let atr_sl_mult = params.get("atr_sl_mult").and_then(|v| v.as_f64()).unwrap_or(2.0);
    let atr_tp_mult = params.get("atr_tp_mult").and_then(|v| v.as_f64()).unwrap_or(3.0);
    let min_trend_weight = params.get("min_trend_weight").and_then(|v| v.as_f64()).unwrap_or(0.6);
    // NEW: ADX threshold for trend strength
    let adx_threshold = params.get("adx_threshold").and_then(|v| v.as_f64()).unwrap_or(20.0);
    
    // Get instrument from params to check against favored
    let instrument = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("XAU_USD");
    let is_gold = instrument.contains("XAU") || instrument.contains("GOLD");
    let is_btc = instrument.contains("BTC");
    
    let n = candles.len();
    let i = n - 1;
    if n < trend_ema + 10 {
        let min_needed = trend_ema + 10;
        let progress = (n as f64 / min_needed as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", n, min_needed))
                .with_target(&format!("{} candles required", min_needed))
                .with_description(&format!("{:.0}% complete — collecting historical data", progress)),
            SignalCondition::new("MRATE Regime", true, &format!("{:?}", mrate.regime))
                .with_target("Any regime")
                .with_description(&format!("Risk mult: {:.2}x", mrate.risk_multiplier)),
            SignalCondition::new("Trend EMA", false, "Pending")
                .with_target(&format!("EMA {} calculated", trend_ema))
                .with_description("Trend direction from EMA"),
            SignalCondition::new("RSI Timing", false, "Pending")
                .with_target(&format!("Long < {:.0} / Short > {:.0}", rsi_long_zone, rsi_short_zone))
                .with_description("RSI zone for entry timing"),
            SignalCondition::new("ADX Strength", false, "Pending")
                .with_target(&format!("> {:.0}", adx_threshold))
                .with_description("Trend strength confirmation"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Initializing: {}/{} candles ({:.0}%)", n, min_needed, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let price = candles[i].close;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: EXTRACT MRATE SIGNALS (USE PER-INSTRUMENT REGIME)
    // ═══════════════════════════════════════════════════════════════════
    let global_regime = &mrate.regime;  // Keep for reference/logging
    let favored = &mrate.favored_instrument;
    let base_trend_weight = mrate.strategy_weights.trend;
    let risk_mult = mrate.risk_multiplier;
    let thresholds = &mrate.thresholds;
    
    // === GET PER-INSTRUMENT DATA ===
    // Each instrument has its own price_regime based on ADX - USE THIS not global!
    let instrument_data = mrate.instrument_scores.get_score(instrument);
    let (instrument_price_regime, instrument_adx, instrument_trend_dir, instrument_score) = match instrument_data {
        Some(data) => (
            data.price_regime,
            data.trend_strength,
            data.trend_direction,
            data.score,
        ),
        None => (
            crate::mrate::models::PriceRegime::Ranging,  // Default safe
            0.0,
            crate::mrate::models::TrendDirection::Flat,
            50.0,
        ),
    };
    
    // Convert per-instrument PriceRegime to trading Regime for this instrument
    // THIS IS THE KEY FIX: Use instrument's own regime, not global!
    use crate::mrate::models::PriceRegime;
    let effective_regime = match instrument_price_regime {
        PriceRegime::Trending => {
            // Instrument is trending - allow trend strategies regardless of global CHOPPY
            Regime::Trend
        },
        PriceRegime::Transitional => {
            // In transition - use global regime as tie-breaker
            global_regime.clone()
        },
        PriceRegime::Ranging => {
            // Instrument is ranging - use Choppy behavior
            Regime::Choppy
        },
    };
    
    // Adjust trend weight based on per-instrument ADX
    let trend_weight = if instrument_price_regime == PriceRegime::Trending && instrument_adx >= 25.0 {
        // Strong per-instrument trend: boost weight regardless of global regime
        let adx_bonus = ((instrument_adx - 25.0) / 50.0).min(0.25); // Up to 25% bonus
        (base_trend_weight + 0.35 + adx_bonus).min(1.0)  // Base boost + ADX bonus
    } else {
        base_trend_weight
    };
    
    // Log when per-instrument regime overrides global
    if effective_regime != *global_regime {
        tracing::info!(
            "🔄 {} per-instrument regime override: Global {:?} → Effective {:?} (ADX: {:.0}, PriceRegime: {:?})",
            instrument, global_regime, effective_regime, instrument_adx, instrument_price_regime
        );
    }
    
    // Get BTC trend from inputs if available
    let btc_trend = mrate.inputs.as_ref().and_then(|i| i.btc_trend);
    let btc_trend_str = btc_trend.map(|t| match t {
        TrendDirection::StrongUp => "StrongUp",
        TrendDirection::Up => "Up",
        TrendDirection::Flat => "Flat",
        TrendDirection::Down => "Down",
        TrendDirection::StrongDown => "StrongDown",
    }).unwrap_or("Unknown");
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: DETERMINE MRATE BIAS
    // ═══════════════════════════════════════════════════════════════════
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum MrateBias { Long, Short, Neutral }
    
    // === DETERMINE BIAS USING EFFECTIVE (PER-INSTRUMENT) REGIME ===
    // Also use instrument_score: LOW score = bearish for LONGS = bullish for SHORTS
    let mrate_bias = match effective_regime {
        Regime::GoldSuperBull => {
            if is_gold { MrateBias::Long } else if is_btc { MrateBias::Neutral } else { MrateBias::Long }
        },
        Regime::BtcSuperBull => {
            if is_btc { MrateBias::Long } else if is_gold { MrateBias::Neutral } else { MrateBias::Long }
        },
        Regime::Panic => {
            // Panic = risk off = gold up, btc down
            if is_gold { MrateBias::Long } else if is_btc { MrateBias::Short } else { MrateBias::Neutral }
        },
        Regime::Trend => {
            // Per-instrument Trend regime: use trend direction + score to determine bias
            // Low score (< 40) = macro unfavorable for LONGS = consider SHORTS
            // High score (> 60) = macro favorable for LONGS
            use crate::mrate::models::TrendDirection;
            
            if instrument_score < 40.0 {
                // Low score = unfavorable for longs = SHORT opportunity if trending down
                match instrument_trend_dir {
                    TrendDirection::Down | TrendDirection::StrongDown => MrateBias::Short,
                    TrendDirection::Up | TrendDirection::StrongUp => MrateBias::Neutral, // Conflicting signals
                    TrendDirection::Flat => MrateBias::Neutral,
                }
            } else if instrument_score > 60.0 {
                // High score = favorable for longs = LONG opportunity if trending up
                match instrument_trend_dir {
                    TrendDirection::Up | TrendDirection::StrongUp => MrateBias::Long,
                    TrendDirection::Down | TrendDirection::StrongDown => MrateBias::Neutral, // Conflicting
                    TrendDirection::Flat => MrateBias::Long,
                }
            } else {
                // Neutral score (40-60): use technical trend direction
                match instrument_trend_dir {
                    TrendDirection::Up | TrendDirection::StrongUp => MrateBias::Long,
                    TrendDirection::Down | TrendDirection::StrongDown => MrateBias::Short,
                    TrendDirection::Flat => MrateBias::Neutral,
                }
            }
        },
        Regime::Choppy => {
            // Effective regime is Choppy (instrument is ranging) - stay neutral
            MrateBias::Neutral
        },
    };
    
    // Also factor in BTC trend for BTC trades
    let btc_bias_override = if is_btc {
        match btc_trend {
            Some(TrendDirection::StrongDown) | Some(TrendDirection::Down) => Some(MrateBias::Short),
            Some(TrendDirection::StrongUp) | Some(TrendDirection::Up) => Some(MrateBias::Long),
            _ => None,
        }
    } else {
        None
    };
    
    let final_bias = btc_bias_override.unwrap_or(mrate_bias);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: TECHNICAL INDICATORS (TIGHTENED)
    // ═══════════════════════════════════════════════════════════════════
    let ema_trend = ema(candles, trend_ema);
    let ema_entry = ema(candles, entry_ema);
    let rsi_values = rsi(candles, rsi_period);
    let atr_values = atr(candles, atr_period);
    let (adx_values, _, _) = adx(candles, 14);
    
    if i >= ema_trend.len() || i >= rsi_values.len() || i >= atr_values.len() || i >= adx_values.len() {
        let conditions = vec![
            SignalCondition::new("MRATE Regime", true, &format!("{:?}", mrate.regime))
                .with_target("Any regime")
                .with_description(&format!("Bias: {:?} | Risk: {:.2}x", final_bias, risk_mult)),
            SignalCondition::new("Technical Indicators", false, "Calculating...")
                .with_target("EMA, RSI, ATR, ADX")
                .with_description("Indicators still computing — need more data"),
        ];
        return SignalResult::hold_with_conditions(
            "📊 Calculating indicators...",
            conditions,
            "NEUTRAL"
        );
    }
    
    let ema_t = ema_trend[i];
    let ema_e = ema_entry[i];
    let rsi_curr = rsi_values[i];
    let rsi_prev = if i > 0 && i - 1 < rsi_values.len() { rsi_values[i - 1] } else { rsi_curr };
    let atr_val = atr_values[i];
    let adx_val = adx_values[i];
    
    // NEW: Check if price is chasing (moved too much in last 3 candles)
    let recent_move = if i >= 3 {
        (candles[i].close - candles[i - 3].close).abs()
    } else {
        0.0
    };
    let is_chasing = recent_move > atr_val * 1.5;  // Reject if moved > 1.5 ATR recently
    
    // Technical conditions (TIGHTENED)
    let tech_uptrend = price > ema_t;
    let tech_downtrend = price < ema_t;
    let near_entry_ema = (price - ema_e).abs() < atr_val * 1.2; // Within 1.2 ATR of entry EMA
    let rsi_not_extreme = rsi_curr > 25.0 && rsi_curr < 75.0;
    let rsi_turning_up = rsi_curr > rsi_prev && rsi_prev < rsi_curr;  // Confirmed turn
    let rsi_turning_down = rsi_curr < rsi_prev && rsi_prev > rsi_curr;  // Confirmed turn
    let adx_ok = adx_val >= adx_threshold;  // NEW: Require trending market
    
    // Apply DTE entry softening to RSI zones (but keep tighter base)
    let adjusted_rsi_long = rsi_long_zone + (thresholds.entry_softening * 5.0);  // Reduced softening
    let adjusted_rsi_short = rsi_short_zone - (thresholds.entry_softening * 5.0);
    
    // TIGHTENED: Remove the loose "momentum" fallback - require actual zone entry
    let rsi_in_long_zone = rsi_curr < adjusted_rsi_long;
    let rsi_in_short_zone = rsi_curr > adjusted_rsi_short;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: BUILD CONDITIONS
    // ═══════════════════════════════════════════════════════════════════
    let direction = match final_bias {
        MrateBias::Long => "LONG",
        MrateBias::Short => "SHORT",
        MrateBias::Neutral => "NEUTRAL",
    };
    
    let bias_str = match final_bias {
        MrateBias::Long => "🟢 LONG",
        MrateBias::Short => "🔴 SHORT",
        MrateBias::Neutral => "⚪ NEUTRAL",
    };
    
    let instrument_match = match favored {
        FavoredInstrument::Gold if is_gold => true,
        FavoredInstrument::Bitcoin if is_btc => true,
        FavoredInstrument::Both => true,
        FavoredInstrument::Neither => false,
        _ => false,
    };
    
    let rsi_target_str = if final_bias == MrateBias::Long {
        format!("< {:.0} or rising", rsi_long_zone)
    } else {
        format!(">{:.0} or falling", rsi_short_zone)
    };
    
    // Show per-instrument regime status
    let regime_status = if effective_regime != *global_regime {
        format!("{:?} (Global: {:?})", effective_regime, global_regime)
    } else {
        format!("{:?}", effective_regime)
    };
    
    let conditions = vec![
        SignalCondition::new(
            "Effective Regime",
            effective_regime != Regime::Choppy,
            &regime_status
        ).with_target("Not CHOPPY")
         .with_description(&format!("Per-instrument ADX: {:.0} | PriceRegime: {:?}", 
            instrument_adx, instrument_price_regime)),
        
        SignalCondition::new(
            "MRATE Bias",
            final_bias != MrateBias::Neutral,
            bias_str
        ).with_description(&format!("Favored: {:?}", favored)),
        
        SignalCondition::new(
            "Instrument Match",
            instrument_match,
            if instrument_match { "✓ Trading favored" } else { "✗ Not favored" }
        ).with_target(&format!("Favored: {:?}", favored)),
        
        SignalCondition::new(
            "Trend Weight",
            trend_weight >= min_trend_weight,
            &format!("{:.0}%", trend_weight * 100.0)
        ).with_target(&format!("≥ {:.0}%", min_trend_weight * 100.0)),
        
        SignalCondition::new(
            &format!("Tech Trend (EMA{})", trend_ema),
            (final_bias == MrateBias::Long && tech_uptrend) || (final_bias == MrateBias::Short && tech_downtrend),
            if tech_uptrend { "UP" } else { "DOWN" }
        ).with_description(&format!("Price ${:.2} vs EMA ${:.2}", price, ema_t)),
        
        SignalCondition::new(
            "RSI Zone",
            rsi_not_extreme && ((final_bias == MrateBias::Long && rsi_in_long_zone) || 
                               (final_bias == MrateBias::Short && rsi_in_short_zone)),
            &format!("{:.1}", rsi_curr)
        ).with_target(&rsi_target_str),
        
        SignalCondition::new(
            &format!("Near Entry (EMA{})", entry_ema),
            near_entry_ema,
            &format!("{:.1} ATR away", (price - ema_e).abs() / atr_val)
        ).with_target("< 1.2 ATR"),
        
        SignalCondition::new(
            "ADX (Trend Strength)",
            adx_ok,
            &format!("{:.1}", adx_val)
        ).with_target(&format!("≥ {:.0}", adx_threshold)),
        
        SignalCondition::new(
            "Not Chasing",
            !is_chasing,
            &format!("{:.1} ATR move", recent_move / atr_val)
        ).with_target("< 1.5 ATR recent move"),
    ];
    
    // Add BTC trend condition if trading BTC
    let mut all_conditions = conditions;
    if is_btc {
        all_conditions.push(
            SignalCondition::new(
                "BTC Trend",
                btc_trend.is_some() && btc_trend != Some(TrendDirection::Flat),
                btc_trend_str
            ).with_description("From MRATE market data")
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: SIGNAL GENERATION (USING PER-INSTRUMENT REGIME)
    // ═══════════════════════════════════════════════════════════════════
    
    // Reject if EFFECTIVE regime (per-instrument) is Choppy
    // This means the INSTRUMENT is ranging (ADX < 20), not that global is choppy
    if effective_regime == Regime::Choppy {
        return SignalResult::hold_with_conditions(
            &format!("⏸️ {} is RANGING (ADX {:.0} < 25) - no trend to trade | Global: {:?}", 
                instrument, instrument_adx, global_regime),
            all_conditions,
            direction
        );
    }
    
    // Reject if no clear bias
    if final_bias == MrateBias::Neutral {
        return SignalResult::hold_with_conditions(
            &format!("⏸️ No clear bias for {} | Score: {:.0} | Trend: {:?}", 
                instrument, instrument_score, instrument_trend_dir),
            all_conditions,
            direction
        );
    }
    
    // Check trend weight threshold
    if trend_weight < min_trend_weight {
        return SignalResult::hold_with_conditions(
            &format!("⏸️ Trend weight {:.0}% < {:.0}% threshold", 
                trend_weight * 100.0, min_trend_weight * 100.0),
            all_conditions,
            direction
        );
    }
    
    // Calculate stops with DTE multipliers
    // Strong markets = tighter stops, extended TP (let winners run)
    let adjusted_sl = thresholds.adjust_stop_loss(atr_sl_mult);
    let adjusted_tp = thresholds.adjust_take_profit(atr_tp_mult);
    
    let sl_price = if final_bias == MrateBias::Long {
        price - (atr_val * adjusted_sl)
    } else {
        price + (atr_val * adjusted_sl)
    };
    let tp_price = if final_bias == MrateBias::Long {
        price + (atr_val * adjusted_tp)
    } else {
        price - (atr_val * adjusted_tp)
    };
    
    // LONG SIGNAL (require ALL conditions)
    if final_bias == MrateBias::Long {
        // Require: tech uptrend + RSI in zone + near entry EMA + ADX trending + not chasing
        if tech_uptrend && rsi_in_long_zone && near_entry_ema && adx_ok && !is_chasing && rsi_turning_up {
            let confidence = 0.60 + (trend_weight * 0.20) + (risk_mult * 0.05);
            let regime_tag = if effective_regime != *global_regime { " [PER-INSTR]" } else { "" };
            return SignalResult::buy_with_conditions(
                confidence.min(0.85),
                &format!("🚀 MRATE LONG{} | {:?} | {} | ADX {:.0} | Score {:.0} | SL {:.1}x TP {:.1}x ATR",
                    regime_tag, effective_regime, thresholds.regime_description(), adx_val, instrument_score, adjusted_sl, adjusted_tp),
                all_conditions
            );
        }
    }
    
    // SHORT SIGNAL (require ALL conditions)
    if final_bias == MrateBias::Short {
        // Require: tech downtrend + RSI in zone + near entry EMA + ADX trending + not chasing
        if tech_downtrend && rsi_in_short_zone && near_entry_ema && adx_ok && !is_chasing && rsi_turning_down {
            let confidence = 0.60 + (trend_weight * 0.20) + (risk_mult * 0.05);
            let regime_tag = if effective_regime != *global_regime { " [PER-INSTR]" } else { "" };
            return SignalResult::sell_with_conditions(
                confidence.min(0.85),
                &format!("🚀 MRATE SHORT{} | {:?} | {} | ADX {:.0} | Score {:.0} | SL {:.1}x TP {:.1}x ATR",
                    regime_tag, effective_regime, thresholds.regime_description(), adx_val, instrument_score, adjusted_sl, adjusted_tp),
                all_conditions
            );
        }
    }
    
    // Hold with current state
    let status = format!(
        "Eff: {:?} (Glb: {:?}) | Bias: {:?} | Trend: {} | RSI: {:.1} | Near EMA: {}",
        effective_regime,
        global_regime,
        final_bias,
        if tech_uptrend { "UP" } else { "DOWN" },
        rsi_curr,
        if near_entry_ema { "✓" } else { "✗" }
    );
    
    SignalResult::hold_with_conditions(
        &format!("⏸️ {} | Waiting for entry", status),
        all_conditions,
        direction
    )
}

/// Real Yield Momentum - Gold's Most Powerful Predictor
/// 
/// This strategy leverages the 10-Year TIPS (Treasury Inflation-Protected Securities) real yield,
/// which is statistically the strongest predictor of gold price movement.
/// 
/// **Why Real Yields Drive Gold:**
/// - Real yield = Nominal yield - Expected inflation
/// - Gold is a zero-yield asset competing with risk-free real yields
/// - When real yields FALL → Opportunity cost of holding gold decreases → Gold UP
/// - When real yields RISE → Better returns from bonds → Gold DOWN
/// 
/// **Entry Logic:**
/// 1. Daily real yield change > 5 basis points (0.05%)
/// 2. MRATE risk score confirms macro environment
/// 3. Technical confirmation (EMA trend + ADX)
/// 
/// **MRATE Category:** `trend` - Follows macro trend direction
fn real_yield_momentum_signal(candles: &[Candle], params: &serde_json::Value, mrate_output: Option<&MrateOutput>) -> SignalResult {
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: Get Real Yield Data from MRATE
    // ═══════════════════════════════════════════════════════════════════
    let mrate = match mrate_output {
        Some(m) => m,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", false, "Unavailable")
                    .with_target("MRATE engine output required")
                    .with_description("This strategy requires MRATE real yield data"),
                SignalCondition::new("Real Yield (10Y TIPS)", false, "Pending")
                    .with_target("FRED API data via MRATE")
                    .with_description("Waiting for MRATE connection"),
                SignalCondition::new("Technical Entry", false, "Pending")
                    .with_description("Waiting for MRATE data"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ Waiting for MRATE data (real yield required)",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    let inputs = match &mrate.inputs {
        Some(i) => i,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", true, "Connected")
                    .with_description(&format!("Regime: {:?}", mrate.regime)),
                SignalCondition::new("MRATE Inputs", false, "Unavailable")
                    .with_target("Market data inputs required")
                    .with_description("MRATE connected but inputs not populated yet"),
                SignalCondition::new("Real Yield (10Y TIPS)", false, "Pending")
                    .with_description("Waiting for MRATE inputs"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ MRATE inputs unavailable",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    let real_yield = match inputs.real_yield_10y {
        Some(y) => y,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", true, "Connected")
                    .with_description(&format!("Regime: {:?}", mrate.regime)),
                SignalCondition::new("MRATE Inputs", true, "Available")
                    .with_description("Market data inputs loaded"),
                SignalCondition::new("Real Yield (10Y TIPS)", false, "No Data")
                    .with_target("FRED API 10Y TIPS yield")
                    .with_description("Real yield data not available from FRED API"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ Real yield data unavailable (FRED API)",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: Configurable Parameters
    // ═══════════════════════════════════════════════════════════════════
    let yield_change_threshold = params.get("yield_change_bps")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);  // 5 basis points (0.05%)
    
    let min_risk_score = params.get("min_risk_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(60.0);  // MRATE risk > 60
    
    let ema_fast = params.get("ema_fast")
        .and_then(|v| v.as_u64())
        .unwrap_or(21) as usize;
    
    let ema_slow = params.get("ema_slow")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;
    
    let adx_threshold = params.get("adx_threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(20.0);
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: Calculate Technical Indicators
    // ═══════════════════════════════════════════════════════════════════
    let i = candles.len() - 1;
    if i < 200 {
        let progress = (i as f64 / 200.0 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/200 candles", i))
                .with_target("200 candles required")
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("Real Yield (10Y TIPS)", true, &format!("{:.2}%", real_yield))
                .with_description("Drives gold opportunity cost"),
            SignalCondition::new("EMA Alignment", false, "Pending")
                .with_target("Need more data for EMA calculation"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Collecting data: {}/200 candles ({:.0}%)", i, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let ema_f = ema(candles, ema_fast);
    let ema_s = ema(candles, ema_slow);
    let (adx_values, _plus_di, _minus_di) = adx(candles, 14);
    
    if i >= ema_f.len() || i >= ema_s.len() || i >= adx_values.len() {
        let conditions = vec![
            SignalCondition::new("Real Yield (10Y TIPS)", true, &format!("{:.2}%", real_yield))
                .with_description("Drives gold opportunity cost"),
            SignalCondition::new("Technical Indicators", false, "Calculating...")
                .with_target("EMA + ADX computation")
                .with_description("Indicators still computing"),
        ];
        return SignalResult::hold_with_conditions("Calculating indicators...", conditions, "NEUTRAL");
    }
    
    let price = candles[i].close;
    let ema_fast_val = ema_f[i];
    let ema_slow_val = ema_s[i];
    let adx_val = adx_values[i];
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: Real Yield Change Detection
    // ═══════════════════════════════════════════════════════════════════
    // We need historical real yield to detect change
    // For now, use MRATE inputs (you may need to store previous value)
    // Approximation: Assume change if real yield is very negative/positive
    
    let yield_falling = real_yield < -0.5;  // Strong negative real yield
    let yield_rising = real_yield > 1.5;    // High positive real yield
    
    // MRATE risk score
    let risk_score = mrate.risk_score;
    let risk_high = risk_score >= min_risk_score;
    
    // Technical confirmation
    let tech_uptrend = price > ema_fast_val && ema_fast_val > ema_slow_val;
    let tech_downtrend = price < ema_fast_val && ema_fast_val < ema_slow_val;
    let trending = adx_val > adx_threshold;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: Build Condition Checklist
    // ═══════════════════════════════════════════════════════════════════
    let mut conditions = vec![
        SignalCondition::new(
            "Real Yield (10Y TIPS)",
            true,
            &format!("{:.2}%", real_yield)
        ).with_description("Drives gold opportunity cost"),
        
        SignalCondition::new(
            "MRATE Risk Score",
            risk_high,
            &format!("{:.0}", risk_score)
        ).with_target(&format!("≥ {:.0}", min_risk_score)),
        
        SignalCondition::new(
            "ADX (Trend Strength)",
            trending,
            &format!("{:.1}", adx_val)
        ).with_target(&format!("≥ {:.0}", adx_threshold)),
        
        SignalCondition::new(
            "EMA Alignment",
            tech_uptrend || tech_downtrend,
            if tech_uptrend { "Bullish" } else if tech_downtrend { "Bearish" } else { "Neutral" }
        ).with_description(&format!("Price ${:.2}, Fast ${:.2}, Slow ${:.2}", price, ema_fast_val, ema_slow_val)),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 6: Signal Generation
    // ═══════════════════════════════════════════════════════════════════
    
    // LONG GOLD: Real yields falling + Risk high + Tech uptrend
    if yield_falling && risk_high && tech_uptrend && trending {
        let confidence = 0.70 + (risk_score / 100.0 * 0.15);
        return SignalResult::buy_with_conditions(
            confidence.min(0.85),
            &format!(
                "🟡 LONG GOLD: Real yield {:.2}% (falling) → Lower opportunity cost | Risk {:.0} | ADX {:.0}",
                real_yield, risk_score, adx_val
            ),
            conditions
        );
    }
    
    // SHORT GOLD: Real yields rising + Risk low + Tech downtrend (RARE but happens)
    if yield_rising && !risk_high && tech_downtrend && trending {
        let confidence = 0.65;  // Lower confidence for gold shorts
        return SignalResult::sell_with_conditions(
            confidence,
            &format!(
                "🟡 SHORT GOLD: Real yield {:.2}% (rising) → Higher opportunity cost | Risk {:.0} | ADX {:.0}",
                real_yield, risk_score, adx_val
            ),
            conditions
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 7: Hold with Status
    // ═══════════════════════════════════════════════════════════════════
    let direction = if tech_uptrend { "LONG" } else { "SHORT" };
    let yield_status = if yield_falling {
        "Falling (Gold bullish)"
    } else if yield_rising {
        "Rising (Gold bearish)"
    } else {
        "Neutral"
    };
    
    SignalResult::hold_with_conditions(
        &format!(
            "⏸️ Real Yield {:.2}% ({}) | Risk {:.0}/{:.0} | ADX {:.0}/{:.0} | Waiting for alignment",
            real_yield, yield_status, risk_score, min_risk_score, adx_val, adx_threshold
        ),
        conditions,
        direction
    )
}

/// Gold-DXY Divergence - Mean Reversion via Correlation Breakdown
/// 
/// **Core Concept**: Gold and the US Dollar Index (DXY) typically have a strong inverse
/// correlation (~-0.85). When this correlation breaks down (divergence), it creates a
/// mean reversion opportunity.
/// 
/// **Divergence Scenarios**:
/// 1. DXY falling + Gold flat/falling → Gold should be rising (LONG gold)
/// 2. DXY rising + Gold flat/rising → Gold should be falling (SHORT gold)
/// 
/// **Entry Logic**:
/// - Detect DXY directional move (EMA trend)
/// - Gold not following the expected inverse move
/// - MRATE risk score confirms macro environment
/// - RSI in mean reversion zone
/// - ADX confirms trending (not choppy)
/// 
/// **MRATE Category:** `mean_reversion` - Trades correlation breakdown reversions
fn gold_dxy_divergence_signal(candles: &[Candle], params: &serde_json::Value, mrate_output: Option<&MrateOutput>) -> SignalResult {
    // ═══════════════════════════════════════════════════════════════════
    // STEP 1: Get DXY Data from MRATE
    // ═══════════════════════════════════════════════════════════════════
    let mrate = match mrate_output {
        Some(m) => m,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", false, "Unavailable")
                    .with_target("MRATE engine output required")
                    .with_description("This strategy requires MRATE DXY data"),
                SignalCondition::new("DXY Level", false, "Pending")
                    .with_target("US Dollar Index from MRATE")
                    .with_description("Waiting for MRATE connection"),
                SignalCondition::new("Divergence Detection", false, "Pending")
                    .with_description("Gold-DXY correlation analysis"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ Waiting for MRATE data (DXY required)",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    let inputs = match &mrate.inputs {
        Some(i) => i,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", true, "Connected")
                    .with_description(&format!("Regime: {:?}", mrate.regime)),
                SignalCondition::new("MRATE Inputs", false, "Unavailable")
                    .with_target("Market data inputs required")
                    .with_description("MRATE connected but inputs not populated yet"),
                SignalCondition::new("DXY Level", false, "Pending")
                    .with_description("Waiting for MRATE inputs"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ MRATE inputs unavailable",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    let dxy_value = match inputs.dxy_level {
        Some(d) => d,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", true, "Connected")
                    .with_description(&format!("Regime: {:?}", mrate.regime)),
                SignalCondition::new("MRATE Inputs", true, "Available")
                    .with_description("Market data inputs loaded"),
                SignalCondition::new("DXY Level", false, "No Data")
                    .with_target("US Dollar Index")
                    .with_description("DXY data not available from market data feeds"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ DXY data unavailable",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 2: Configurable Parameters
    // ═══════════════════════════════════════════════════════════════════
    let lookback = params.get("lookback_period")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    
    let min_risk_score = params.get("min_risk_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);
    
    let ema_period = params.get("ema_period")
        .and_then(|v| v.as_u64())
        .unwrap_or(21) as usize;
    
    let rsi_period = params.get("rsi_period")
        .and_then(|v| v.as_u64())
        .unwrap_or(14) as usize;
    
    let adx_threshold = params.get("adx_threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(20.0);
    
    let divergence_threshold = params.get("divergence_threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5); // 0.5% move threshold
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 3: Calculate Technical Indicators
    // ═══════════════════════════════════════════════════════════════════
    let i = candles.len() - 1;
    if i < lookback + 50 {
        let min_needed = lookback + 50;
        let progress = (i as f64 / min_needed as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, min_needed))
                .with_target(&format!("{} candles required", min_needed))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("DXY Level", true, &format!("{:.2}", dxy_value))
                .with_description("US Dollar Index"),
            SignalCondition::new("Divergence Detection", false, "Pending")
                .with_description("Need more gold price history for correlation analysis"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Collecting data: {}/{} candles ({:.0}%)", i, min_needed, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    let ema_vals = ema(candles, ema_period);
    let rsi_vals = rsi(candles, rsi_period);
    let (adx_vals, _plus_di, _minus_di) = adx(candles, 14);
    
    if i >= ema_vals.len() || i >= rsi_vals.len() || i >= adx_vals.len() {
        let conditions = vec![
            SignalCondition::new("DXY Level", true, &format!("{:.2}", dxy_value))
                .with_description("US Dollar Index"),
            SignalCondition::new("Technical Indicators", false, "Calculating...")
                .with_target("EMA + RSI + ADX computation")
                .with_description("Indicators still computing"),
        ];
        return SignalResult::hold_with_conditions("Calculating indicators...", conditions, "NEUTRAL");
    }
    
    let price = candles[i].close;
    let price_prev = candles[i - lookback].close;
    let ema_val = ema_vals[i];
    let rsi_val = rsi_vals[i];
    let adx_val = adx_vals[i];
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 4: Detect Gold and DXY Directional Moves
    // ═══════════════════════════════════════════════════════════════════
    
    // Gold % change over lookback period
    let gold_change_pct = ((price - price_prev) / price_prev) * 100.0;
    
    // DXY direction (assume we have recent DXY trend data)
    // For simplicity, we'll use MRATE's DXY level and compare to EMA
    // In production, you'd track DXY historical data
    let dxy_rising = dxy_value > 103.0; // Simplified: DXY > 103 = rising
    let dxy_falling = dxy_value < 102.0; // DXY < 102 = falling
    
    // Gold expected behavior given DXY move
    // If DXY rising → Gold should fall (inverse correlation)
    // If DXY falling → Gold should rise
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 5: Detect Divergence
    // ═══════════════════════════════════════════════════════════════════
    
    // Divergence Type 1: DXY falling but Gold also flat/falling (should be rising!)
    let bullish_divergence = dxy_falling && gold_change_pct < divergence_threshold;
    
    // Divergence Type 2: DXY rising but Gold also flat/rising (should be falling!)
    let bearish_divergence = dxy_rising && gold_change_pct > -divergence_threshold;
    
    // MRATE confirmation
    let risk_score = mrate.risk_score;
    let risk_adequate = risk_score >= min_risk_score;
    
    // Technical confirmation
    let price_above_ema = price > ema_val;
    let price_below_ema = price < ema_val;
    let adx_trending = adx_val > adx_threshold;
    let rsi_oversold = rsi_val < 40.0;
    let rsi_overbought = rsi_val > 60.0;
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 6: Build Condition Checklist
    // ═══════════════════════════════════════════════════════════════════
    let mut conditions = vec![
        SignalCondition::new(
            "DXY Level",
            true,
            &format!("{:.2}", dxy_value)
        ).with_description(if dxy_rising { "Rising (Gold bearish)" } else if dxy_falling { "Falling (Gold bullish)" } else { "Neutral" }),
        
        SignalCondition::new(
            "Gold Change",
            true,
            &format!("{:+.2}%", gold_change_pct)
        ).with_description(&format!("{} period", lookback)),
        
        SignalCondition::new(
            "Divergence Detected",
            bullish_divergence || bearish_divergence,
            if bullish_divergence { "Bullish (DXY↓ + Gold↓)" } else if bearish_divergence { "Bearish (DXY↑ + Gold↑)" } else { "None" }
        ).with_description("Correlation breakdown"),
        
        SignalCondition::new(
            "MRATE Risk Score",
            risk_adequate,
            &format!("{:.0}", risk_score)
        ).with_target(&format!("≥ {:.0}", min_risk_score)),
        
        SignalCondition::new(
            "RSI",
            true,
            &format!("{:.1}", rsi_val)
        ).with_description(if rsi_oversold { "Oversold" } else if rsi_overbought { "Overbought" } else { "Neutral" }),
        
        SignalCondition::new(
            "ADX (Trend Strength)",
            adx_trending,
            &format!("{:.1}", adx_val)
        ).with_target(&format!("≥ {:.0}", adx_threshold)),
    ];
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 7: Signal Generation
    // ═══════════════════════════════════════════════════════════════════
    
    // LONG GOLD: Bullish divergence + oversold RSI + price near/above EMA
    if bullish_divergence && risk_adequate && (rsi_oversold || price_above_ema) && adx_trending {
        let confidence: f64 = 0.65 + (if rsi_oversold { 0.10 } else { 0.0 }) + (if price_above_ema { 0.05 } else { 0.0 });
        return SignalResult::buy_with_conditions(
            confidence.min(0.80),
            &format!(
                "📉📈 LONG GOLD: DXY falling ({:.2}) but Gold weak ({:+.1}%) → Divergence reversion | RSI {:.0}",
                dxy_value, gold_change_pct, rsi_val
            ),
            conditions
        );
    }
    
    // SHORT GOLD: Bearish divergence + overbought RSI + price near/below EMA
    if bearish_divergence && risk_adequate && (rsi_overbought || price_below_ema) && adx_trending {
        let confidence: f64 = 0.60 + (if rsi_overbought { 0.10 } else { 0.0 }) + (if price_below_ema { 0.05 } else { 0.0 });
        return SignalResult::sell_with_conditions(
            confidence.min(0.75),
            &format!(
                "📈📉 SHORT GOLD: DXY rising ({:.2}) but Gold strong ({:+.1}%) → Divergence reversion | RSI {:.0}",
                dxy_value, gold_change_pct, rsi_val
            ),
            conditions
        );
    }
    
    // ═══════════════════════════════════════════════════════════════════
    // STEP 8: Hold with Status
    // ═══════════════════════════════════════════════════════════════════
    let direction = if gold_change_pct > 0.0 { "SHORT" } else { "LONG" };
    let correlation_status = if bullish_divergence || bearish_divergence {
        "Divergence detected"
    } else {
        "Normal correlation"
    };
    
    SignalResult::hold_with_conditions(
        &format!(
            "⏸️ DXY {:.2} | Gold {:+.1}% | {} | Risk {:.0}/{:.0} | RSI {:.0} | Waiting for entry",
            dxy_value, gold_change_pct, correlation_status, risk_score, min_risk_score, rsi_val
        ),
        conditions,
        direction
    )
}

// ═══════════════════════════════════════════════════════════════════════════════
// FOMC VOLATILITY BREAKOUT
// ═══════════════════════════════════════════════════════════════════════════════
// Concept: Trade volatility expansion around FOMC meetings
// - Pre-event: Market compresses (low ATR)
// - Post-event: Explosive breakout in either direction
// - Strategy: Enter breakouts 0-8 hours after FOMC with momentum confirmation
// Category: breakout
// MRATE Integration: Uses calendar event detection + uncertainty score
// ═══════════════════════════════════════════════════════════════════════════════

fn fomc_volatility_signal(
    candles: &[Candle],
    params: &serde_json::Value,
    mrate_output: Option<&MrateOutput>,
) -> SignalResult {
    // STEP 1: Extract parameters
    let pre_event_hours = params["pre_event_hours"].as_f64().unwrap_or(12.0);
    let post_event_hours = params["post_event_hours"].as_f64().unwrap_or(8.0);
    let breakout_threshold = params["breakout_threshold"].as_f64().unwrap_or(0.3); // % move
    let min_uncertainty = params["min_uncertainty"].as_f64().unwrap_or(60.0);
    let atr_period = params["atr_period"].as_u64().unwrap_or(14) as usize;
    let rsi_period = params["rsi_period"].as_u64().unwrap_or(14) as usize;
    
    let mrate = match mrate_output {
        Some(m) => m,
        None => {
            let conditions = vec![
                SignalCondition::new("MRATE Data", false, "Unavailable")
                    .with_target("MRATE engine output required")
                    .with_description("This strategy requires MRATE event calendar data"),
                SignalCondition::new("High-Impact Event", false, "Pending")
                    .with_target("FOMC / NFP / CPI detection")
                    .with_description("Waiting for MRATE connection"),
                SignalCondition::new("Breakout Detection", false, "Pending")
                    .with_description("Waiting for event window"),
            ];
            return SignalResult::hold_with_conditions(
                "⏸️ FOMC Volatility: MRATE data unavailable",
                conditions,
                "NEUTRAL"
            );
        }
    };
    
    let i = candles.len() - 1;
    if i < atr_period.max(rsi_period) + 20 {
        let min_needed = atr_period.max(rsi_period) + 20;
        let progress = (i as f64 / min_needed as f64 * 100.0).min(99.0);
        let conditions = vec![
            SignalCondition::new("Data Collection", false, &format!("{}/{} candles", i, min_needed))
                .with_target(&format!("{} candles required", min_needed))
                .with_description(&format!("{:.0}% complete", progress)),
            SignalCondition::new("MRATE Data", true, &format!("{:?}", mrate.regime))
                .with_description("MRATE connected"),
            SignalCondition::new("ATR Volatility", false, "Pending")
                .with_description("Need more candles for ATR calculation"),
        ];
        return SignalResult::hold_with_conditions(
            &format!("📊 Collecting data: {}/{} candles ({:.0}%)", i, min_needed, progress),
            conditions,
            "NEUTRAL"
        );
    }
    
    // STEP 2: Check for high-impact event timing
    let hours_to_event = mrate.inputs.as_ref().and_then(|i| i.hours_to_next_high_impact);
    let event_within_window = hours_to_event.is_some();
    
    let in_pre_event_window = hours_to_event.map(|h| h > 0.0 && h <= pre_event_hours).unwrap_or(false);
    let in_post_event_window = hours_to_event.map(|h| h < 0.0 && h.abs() <= post_event_hours).unwrap_or(false);
    
    // STEP 3: Calculate volatility metrics
    let atr_vals = atr(candles, atr_period);
    let atr_current = atr_vals[i];
    
    // ATR percentile (compare to recent history)
    let lookback = 50.min(i);
    let recent_atrs: Vec<f64> = atr_vals.iter().rev().take(lookback).copied().collect();
    let atr_percentile = recent_atrs.iter().filter(|&&x| x < atr_current).count() as f64 / recent_atrs.len() as f64 * 100.0;
    
    // STEP 4: Calculate momentum indicators
    let rsi_vals = rsi(candles, rsi_period);
    let rsi_val = rsi_vals[i];
    
    let ema_fast_vals = ema(candles, 9);
    let ema_slow_vals = ema(candles, 21);
    let ema_fast = ema_fast_vals[i];
    let ema_slow = ema_slow_vals[i];
    
    let price = candles[i].close;
    let price_prev = candles[i.saturating_sub(1)].close;
    let price_change_pct = ((price - price_prev) / price_prev * 100.0).abs();
    
    // STEP 5: Detect breakout
    let is_breakout = price_change_pct >= breakout_threshold;
    let bullish_momentum = ema_fast > ema_slow && price > ema_fast;
    let bearish_momentum = ema_fast < ema_slow && price < ema_fast;
    
    // STEP 6: MRATE confirmation
    let uncertainty_score = mrate.uncertainty_score;
    let uncertainty_high = uncertainty_score >= min_uncertainty;
    
    // STEP 7: Build condition checklist
    let event_status = if let Some(h) = hours_to_event {
        if h > 0.0 {
            format!("In {:.1}h (pre-event)", h)
        } else {
            format!("{:.1}h ago (post-event)", h.abs())
        }
    } else {
        "No event detected".to_string()
    };
    
    let mut conditions = vec![
        SignalCondition::new(
            "High-Impact Event",
            event_within_window,
            &event_status
        ).with_description("FOMC, NFP, CPI detected"),
        
        SignalCondition::new(
            "Event Window",
            in_post_event_window,
            if in_post_event_window { "Post-event" } else if in_pre_event_window { "Pre-event (wait)" } else { "Outside window" }
        ).with_target(&format!("0-{}h after event", post_event_hours)),
        
        SignalCondition::new(
            "MRATE Uncertainty",
            uncertainty_high,
            &format!("{:.0}", uncertainty_score)
        ).with_target(&format!("≥ {:.0}", min_uncertainty))
        .with_description("Pre-event tension"),
        
        SignalCondition::new(
            "ATR Percentile",
            true,
            &format!("{:.0}%", atr_percentile)
        ).with_description(if atr_percentile > 70.0 { "High volatility" } else if atr_percentile < 30.0 { "Compressed" } else { "Normal" }),
        
        SignalCondition::new(
            "Breakout Size",
            is_breakout,
            &format!("{:.2}%", price_change_pct)
        ).with_target(&format!("≥ {:.1}%", breakout_threshold)),
        
        SignalCondition::new(
            "RSI",
            true,
            &format!("{:.1}", rsi_val)
        ).with_description(if rsi_val > 70.0 { "Overbought" } else if rsi_val < 30.0 { "Oversold" } else { "Neutral" }),
        
        SignalCondition::new(
            "EMA Alignment",
            bullish_momentum || bearish_momentum,
            if bullish_momentum { "Bullish (9>21)" } else if bearish_momentum { "Bearish (9<21)" } else { "Mixed" }
        ),
    ];
    
    // STEP 8: Signal generation
    // Only trade in post-event window with breakout + momentum + uncertainty
    
    if in_post_event_window && is_breakout && uncertainty_high {
        // LONG: Bullish breakout post-event
        if bullish_momentum && rsi_val < 75.0 {
            let confidence: f64 = 0.70 + (if atr_percentile > 80.0 { 0.10 } else { 0.0 }) + (if rsi_val > 60.0 { 0.05 } else { 0.0 });
            return SignalResult::buy_with_conditions(
                confidence.min(0.85),
                &format!(
                    "💥⬆️ FOMC LONG: Post-event breakout +{:.2}% | {} | ATR {:.0}%ile | RSI {:.0}",
                    price_change_pct, 
                    event_status,
                    atr_percentile,
                    rsi_val
                ),
                conditions
            );
        }
        
        // SHORT: Bearish breakout post-event
        if bearish_momentum && rsi_val > 25.0 {
            let confidence: f64 = 0.65 + (if atr_percentile > 80.0 { 0.10 } else { 0.0 }) + (if rsi_val < 40.0 { 0.05 } else { 0.0 });
            return SignalResult::sell_with_conditions(
                confidence.min(0.80),
                &format!(
                    "💥⬇️ FOMC SHORT: Post-event breakout -{:.2}% | {} | ATR {:.0}%ile | RSI {:.0}",
                    price_change_pct,
                    event_status,
                    atr_percentile,
                    rsi_val
                ),
                conditions
            );
        }
    }
    
    // STEP 9: Hold states
    let direction = if price > ema_slow { "LONG" } else { "SHORT" };
    
    if in_pre_event_window {
        return SignalResult::hold_with_conditions(
            &format!(
                "⏳ FOMC PRE-EVENT: {} | Uncertainty {:.0} | ATR {:.0}%ile | Waiting for event",
                event_status, uncertainty_score, atr_percentile
            ),
            conditions,
            direction
        );
    }
    
    if in_post_event_window && !is_breakout {
        return SignalResult::hold_with_conditions(
            &format!(
                "⏸️ FOMC POST-EVENT: {} | Move {:.2}% (need {:.1}%) | Waiting for breakout",
                event_status, price_change_pct, breakout_threshold
            ),
            conditions,
            direction
        );
    }
    
    SignalResult::hold_with_conditions(
        &format!(
            "⏸️ No FOMC event window | {} | ATR {:.0}%ile | Uncertainty {:.0}",
            if event_within_window { event_status } else { "No upcoming events".to_string() },
            atr_percentile,
            uncertainty_score
        ),
        conditions,
        direction
    )
}
