-- Macro-Aligned Momentum (MAM) Strategy
-- Institutional-style strategy combining Polymarket macro sentiment with technical momentum
-- Only trades when macro regime aligns with technical direction
-- Position sizing scales with macro conviction

-- Gold MAM Strategy (4H timeframe)
INSERT INTO strategies (id, user_id, name, strategy_type, params, is_active, risk)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Macro-Aligned Momentum',
    'macro_aligned_momentum',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H4",
        "trend_ema": 200,
        "value_ema": 21,
        "rsi_period": 14,
        "rsi_oversold": 35,
        "rsi_overbought": 65,
        "adx_threshold": 20,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "breakout_period": 20,
        "macro_threshold": 15,
        "strong_threshold": 25,
        "very_strong_threshold": 40,
        "breakout_duration_days": 10
    }',
    false,
    '{
        "stop_loss_pct": 1.5,
        "take_profit_pct": 4.5,
        "position_size_pct": 2.0,
        "max_daily_loss_pct": 5.0
    }'
) ON CONFLICT (name) DO UPDATE SET
    strategy_type = EXCLUDED.strategy_type,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

-- Bitcoin MAM Strategy (4H timeframe)
-- Note: For BTC, RISK_OFF is less negative (safe haven for crypto believers)
INSERT INTO strategies (id, user_id, name, strategy_type, params, is_active, risk)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Bitcoin Macro-Aligned Momentum',
    'macro_aligned_momentum',
    '{
        "symbol": "BTC_USD",
        "timeframe": "H4",
        "trend_ema": 200,
        "value_ema": 21,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "adx_threshold": 18,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "breakout_period": 20,
        "macro_threshold": 15,
        "strong_threshold": 25,
        "very_strong_threshold": 40,
        "breakout_duration_days": 10
    }',
    false,
    '{
        "stop_loss_pct": 2.0,
        "take_profit_pct": 6.0,
        "position_size_pct": 1.5,
        "max_daily_loss_pct": 5.0
    }'
) ON CONFLICT (name) DO UPDATE SET
    strategy_type = EXCLUDED.strategy_type,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

-- Gold Daily MAM Strategy (for swing traders)
INSERT INTO strategies (id, user_id, name, strategy_type, params, is_active, risk)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Daily Macro Swing',
    'macro_aligned_momentum',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "trend_ema": 200,
        "value_ema": 21,
        "rsi_period": 14,
        "rsi_oversold": 40,
        "rsi_overbought": 60,
        "adx_threshold": 15,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "breakout_period": 20,
        "macro_threshold": 15,
        "strong_threshold": 25,
        "very_strong_threshold": 40,
        "breakout_duration_days": 10
    }',
    false,
    '{
        "stop_loss_pct": 2.5,
        "take_profit_pct": 7.5,
        "position_size_pct": 1.5,
        "max_daily_loss_pct": 5.0
    }'
) ON CONFLICT (name) DO UPDATE SET
    strategy_type = EXCLUDED.strategy_type,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;
