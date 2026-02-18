-- Add Volatility Expansion (Squeeze Breakout) Strategy

INSERT INTO strategies (
    id,
    user_id,
    name,
    strategy_type,
    params,
    risk,
    is_active
) VALUES (
    'c0000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000001',
    'Volatility Expansion (Daily)',
    'volatility_expansion',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "description": "Black-Scholes inspired squeeze trading: detects volatility compression, waits for breakout. Wide ATR stops for big moves.",
        "vol_period": 20,
        "baseline_period": 90,
        "compression_ratio": 0.6,
        "bb_period": 20,
        "bb_std": 2.0,
        "bw_lookback": 120,
        "breakout_period": 20,
        "atr_period": 14,
        "atr_sl_mult": 2.5,
        "atr_tp_mult": 5.0,
        "vol_confirm": 1.0
    }',
    '{
        "stop_loss_pct": 2.5,
        "take_profit_pct": 5.0,
        "position_size_pct": 1.0,
        "max_daily_loss_pct": 5.0
    }',
    true
);
