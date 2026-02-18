//! Technical indicators for trading strategies

use crate::models::Candle;

/// Simple Moving Average
pub fn sma(candles: &[Candle], period: usize) -> Vec<f64> {
    if period == 0 || candles.len() < period {
        return vec![0.0; candles.len()]; // Return zeros to avoid index errors
    }
    
    let mut result = Vec::with_capacity(candles.len());
    
    // Pad with zeros for the warmup period
    for _ in 0..(period - 1) {
        result.push(0.0);
    }
    
    // Calculate SMA for the rest
    for i in (period - 1)..candles.len() {
        let start = i + 1 - period;
        let sum: f64 = candles[start..=i]
            .iter()
            .map(|c| c.close)
            .sum();
        result.push(sum / period as f64);
    }
    
    result
}

/// Exponential Moving Average
pub fn ema(candles: &[Candle], period: usize) -> Vec<f64> {
    if candles.is_empty() || period == 0 {
        return vec![];
    }
    
    let mut result = Vec::with_capacity(candles.len());
    let multiplier = 2.0 / (period as f64 + 1.0);
    
    // Start with SMA for the first value
    let initial_sma: f64 = candles[..period.min(candles.len())]
        .iter()
        .map(|c| c.close)
        .sum::<f64>() / period.min(candles.len()) as f64;
    
    result.push(initial_sma);
    
    for candle in candles.iter().skip(1) {
        let prev_ema = result.last().unwrap();
        let new_ema = (candle.close - prev_ema) * multiplier + prev_ema;
        result.push(new_ema);
    }
    
    result
}

/// Relative Strength Index
pub fn rsi(candles: &[Candle], period: usize) -> Vec<f64> {
    if period == 0 || candles.len() < period + 1 {
        return vec![50.0; candles.len()]; // Return neutral RSI to avoid index errors
    }
    
    let mut gains = Vec::new();
    let mut losses = Vec::new();
    
    for i in 1..candles.len() {
        let change = candles[i].close - candles[i - 1].close;
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(change.abs());
        }
    }
    
    // Pad with neutral RSI for warmup
    let mut result = vec![50.0; period + 1];
    
    if gains.len() < period {
        return vec![50.0; candles.len()];
    }
    
    // Calculate initial average gain and loss
    let mut avg_gain: f64 = gains[..period].iter().sum::<f64>() / period as f64;
    let mut avg_loss: f64 = losses[..period].iter().sum::<f64>() / period as f64;
    
    for i in period..gains.len() {
        avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;
        
        let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
        let rsi = 100.0 - (100.0 / (1.0 + rs));
        result.push(rsi);
    }
    
    result
}

/// Bollinger Bands
pub fn bollinger_bands(candles: &[Candle], period: usize, std_dev_mult: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let sma_values = sma(candles, period);
    
    let mut upper = Vec::new();
    let mut middle = Vec::new();
    let mut lower = Vec::new();
    
    for (i, sma_val) in sma_values.iter().enumerate() {
        let start = i;
        let end = i + period;
        
        if end > candles.len() {
            break;
        }
        
        let variance: f64 = candles[start..end]
            .iter()
            .map(|c| (c.close - sma_val).powi(2))
            .sum::<f64>() / period as f64;
        
        let std_dev = variance.sqrt();
        
        middle.push(*sma_val);
        upper.push(sma_val + std_dev * std_dev_mult);
        lower.push(sma_val - std_dev * std_dev_mult);
    }
    
    (upper, middle, lower)
}

/// Average True Range
pub fn atr(candles: &[Candle], period: usize) -> Vec<f64> {
    if period == 0 || candles.len() < 2 {
        return vec![0.0; candles.len()];
    }
    
    let mut true_ranges = Vec::new();
    
    for i in 1..candles.len() {
        let high_low = candles[i].high - candles[i].low;
        let high_close = (candles[i].high - candles[i - 1].close).abs();
        let low_close = (candles[i].low - candles[i - 1].close).abs();
        
        let tr = high_low.max(high_close).max(low_close);
        true_ranges.push(tr);
    }
    
    if true_ranges.len() < period {
        return vec![0.0; candles.len()];
    }
    
    // Pad with zeros for warmup
    let mut result = vec![0.0; period];
    
    // Calculate ATR using SMA of true ranges
    for i in (period - 1)..true_ranges.len() {
        let start = i + 1 - period;
        let sum: f64 = true_ranges[start..=i].iter().sum();
        result.push(sum / period as f64);
    }
    
    result
}

/// MACD (Moving Average Convergence Divergence)
/// Returns (macd_line, signal_line, histogram)
pub fn macd(candles: &[Candle], fast: usize, slow: usize, signal: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let fast_ema = ema(candles, fast);
    let slow_ema = ema(candles, slow);
    
    // MACD line = fast EMA - slow EMA
    let macd_line: Vec<f64> = fast_ema.iter()
        .zip(slow_ema.iter())
        .map(|(f, s)| f - s)
        .collect();
    
    // Signal line = EMA of MACD line
    let multiplier = 2.0 / (signal as f64 + 1.0);
    let mut signal_line = Vec::with_capacity(macd_line.len());
    
    if !macd_line.is_empty() {
        signal_line.push(macd_line[0]);
        for i in 1..macd_line.len() {
            let prev = signal_line[i - 1];
            let new_val = (macd_line[i] - prev) * multiplier + prev;
            signal_line.push(new_val);
        }
    }
    
    // Histogram = MACD line - Signal line
    let histogram: Vec<f64> = macd_line.iter()
        .zip(signal_line.iter())
        .map(|(m, s)| m - s)
        .collect();
    
    (macd_line, signal_line, histogram)
}

/// Stochastic Oscillator
/// Returns (%K, %D)
pub fn stochastic(candles: &[Candle], k_period: usize, d_period: usize) -> (Vec<f64>, Vec<f64>) {
    if candles.len() < k_period {
        return (vec![50.0; candles.len()], vec![50.0; candles.len()]);
    }
    
    let mut k_values = vec![50.0; k_period - 1];
    
    for i in (k_period - 1)..candles.len() {
        let start = i + 1 - k_period;
        let highest = candles[start..=i].iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
        let lowest = candles[start..=i].iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        
        let k = if highest == lowest {
            50.0
        } else {
            ((candles[i].close - lowest) / (highest - lowest)) * 100.0
        };
        k_values.push(k);
    }
    
    // %D is SMA of %K
    let mut d_values = vec![50.0; d_period - 1];
    for i in (d_period - 1)..k_values.len() {
        let start = i + 1 - d_period;
        let sum: f64 = k_values[start..=i].iter().sum();
        d_values.push(sum / d_period as f64);
    }
    
    // Pad d_values to match k_values length
    while d_values.len() < k_values.len() {
        d_values.push(*d_values.last().unwrap_or(&50.0));
    }
    
    (k_values, d_values)
}

/// ADX (Average Directional Index) with +DI and -DI
/// Returns (adx, plus_di, minus_di)
pub fn adx(candles: &[Candle], period: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    if candles.len() < period + 1 {
        let len = candles.len();
        return (vec![0.0; len], vec![0.0; len], vec![0.0; len]);
    }
    
    let mut plus_dm = Vec::new();
    let mut minus_dm = Vec::new();
    let mut tr = Vec::new();
    
    for i in 1..candles.len() {
        let high_diff = candles[i].high - candles[i - 1].high;
        let low_diff = candles[i - 1].low - candles[i].low;
        
        // +DM
        if high_diff > low_diff && high_diff > 0.0 {
            plus_dm.push(high_diff);
        } else {
            plus_dm.push(0.0);
        }
        
        // -DM
        if low_diff > high_diff && low_diff > 0.0 {
            minus_dm.push(low_diff);
        } else {
            minus_dm.push(0.0);
        }
        
        // True Range
        let high_low = candles[i].high - candles[i].low;
        let high_close = (candles[i].high - candles[i - 1].close).abs();
        let low_close = (candles[i].low - candles[i - 1].close).abs();
        tr.push(high_low.max(high_close).max(low_close));
    }
    
    // Smooth the values using Wilder's smoothing
    let smooth = |values: &[f64], period: usize| -> Vec<f64> {
        if values.len() < period {
            return vec![0.0; values.len()];
        }
        let mut result = vec![0.0; period - 1];
        let initial: f64 = values[..period].iter().sum();
        result.push(initial);
        for i in period..values.len() {
            let prev = result[i - 1];
            let smoothed = prev - (prev / period as f64) + values[i];
            result.push(smoothed);
        }
        result
    };
    
    let smoothed_plus_dm = smooth(&plus_dm, period);
    let smoothed_minus_dm = smooth(&minus_dm, period);
    let smoothed_tr = smooth(&tr, period);
    
    // Calculate +DI and -DI
    let plus_di: Vec<f64> = smoothed_plus_dm.iter()
        .zip(smoothed_tr.iter())
        .map(|(dm, tr)| if *tr == 0.0 { 0.0 } else { (dm / tr) * 100.0 })
        .collect();
    
    let minus_di: Vec<f64> = smoothed_minus_dm.iter()
        .zip(smoothed_tr.iter())
        .map(|(dm, tr)| if *tr == 0.0 { 0.0 } else { (dm / tr) * 100.0 })
        .collect();
    
    // Calculate DX
    let dx: Vec<f64> = plus_di.iter()
        .zip(minus_di.iter())
        .map(|(p, m)| {
            let sum = p + m;
            if sum == 0.0 { 0.0 } else { ((p - m).abs() / sum) * 100.0 }
        })
        .collect();
    
    // ADX = smoothed DX
    let adx_values = smooth(&dx, period);
    let adx_normalized: Vec<f64> = adx_values.iter()
        .map(|v| v / period as f64)
        .collect();
    
    // Pad to match original candles length
    let mut final_adx = vec![0.0];
    final_adx.extend(adx_normalized);
    let mut final_plus_di = vec![0.0];
    final_plus_di.extend(plus_di);
    let mut final_minus_di = vec![0.0];
    final_minus_di.extend(minus_di);
    
    (final_adx, final_plus_di, final_minus_di)
}

/// Donchian Channel
/// Returns (upper, middle, lower)
pub fn donchian(candles: &[Candle], period: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    if candles.len() < period {
        let len = candles.len();
        return (vec![0.0; len], vec![0.0; len], vec![0.0; len]);
    }
    
    let mut upper = vec![0.0; period - 1];
    let mut lower = vec![0.0; period - 1];
    let mut middle = vec![0.0; period - 1];
    
    for i in (period - 1)..candles.len() {
        let start = i + 1 - period;
        let highest = candles[start..=i].iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
        let lowest = candles[start..=i].iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        
        upper.push(highest);
        lower.push(lowest);
        middle.push((highest + lowest) / 2.0);
    }
    
    (upper, middle, lower)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_candles() -> Vec<Candle> {
        vec![
            Candle { time: 1, open: 100.0, high: 105.0, low: 95.0, close: 102.0, volume: None },
            Candle { time: 2, open: 102.0, high: 108.0, low: 100.0, close: 105.0, volume: None },
            Candle { time: 3, open: 105.0, high: 110.0, low: 103.0, close: 108.0, volume: None },
            Candle { time: 4, open: 108.0, high: 112.0, low: 106.0, close: 110.0, volume: None },
            Candle { time: 5, open: 110.0, high: 115.0, low: 108.0, close: 112.0, volume: None },
        ]
    }
    
    #[test]
    fn test_sma() {
        let candles = create_test_candles();
        let result = sma(&candles, 3);
        assert_eq!(result.len(), 3);
    }
    
    #[test]
    fn test_ema() {
        let candles = create_test_candles();
        let result = ema(&candles, 3);
        assert_eq!(result.len(), candles.len());
    }
}
