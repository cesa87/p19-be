-- Add London/NY Trend Continuation and Forecast Confidence strategies

-- London/NY Trend Continuation Strategy
INSERT INTO strategies (
    id,
    user_id,
    name,
    strategy_type,
    params,
    risk,
    is_active
) VALUES (
    'c0000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'London/NY Trend Continuation',
    'london_ny_trend_continuation',
    '{
        "symbol": "XAU_USD",
        "timeframe": "M15",
        "description": "Professional session-based strategy: London/NY hours only + EMA200 trend + ADX + pullback zone + momentum trigger",
        "session_start": 7,
        "session_end": 16,
        "trend_ema": 200,
        "fast_ema": 21,
        "slow_ema": 50,
        "rsi_period": 14,
        "atr_period": 14,
        "adx_threshold": 20,
        "atr_zone_mult": 1.2,
        "vol_low": 0.7,
        "vol_high": 1.8,
        "rsi_long_trigger": 45,
        "rsi_short_trigger": 55
    }',
    '{
        "stop_loss_pct": 1.5,
        "take_profit_pct": 3.0,
        "position_size_pct": 0.5,
        "max_daily_loss_pct": 3.0
    }',
    true
);

-- Forecast Confidence (Holt-Winters) Strategy
INSERT INTO strategies (
    id,
    user_id,
    name,
    strategy_type,
    params,
    risk,
    is_active
) VALUES (
    'c0000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000001',
    'Forecast Confidence (Daily)',
    'forecast_confidence',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "description": "Holt-Winters statistical forecasting: predicts next day price, trades only when model accuracy is high. Swing trading on Daily timeframe.",
        "hw_alpha": 0.3,
        "hw_beta": 0.1,
        "forecast_threshold": 0.3,
        "accuracy_window": 30,
        "min_accuracy": 55,
        "rsi_period": 14,
        "atr_period": 14,
        "atr_sl_mult": 1.8,
        "atr_tp_mult": 3.6
    }',
    '{
        "stop_loss_pct": 1.8,
        "take_profit_pct": 3.6,
        "position_size_pct": 1.0,
        "max_daily_loss_pct": 3.0
    }',
    true
);
