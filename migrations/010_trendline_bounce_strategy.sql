-- Add Trendline Bounce Strategy

INSERT INTO strategies (
    id,
    user_id,
    name,
    strategy_type,
    params,
    risk,
    is_active
) VALUES (
    'c0000000-0000-0000-0000-000000000004',
    'a0000000-0000-0000-0000-000000000001',
    'Trendline Bounce',
    'trendline_bounce',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H1",
        "description": "Structure trading: auto-detects swing points, builds trendlines via regression, trades bounces with candlestick confirmation.",
        "swing_lookback": 5,
        "min_swing_points": 2,
        "min_slope": 0.05,
        "adx_threshold": 20,
        "touch_atr_mult": 0.5,
        "atr_period": 14,
        "atr_sl_mult": 1.0,
        "atr_tp_mult": 2.5,
        "ema_trail_period": 21
    }',
    '{
        "stop_loss_pct": 1.0,
        "take_profit_pct": 2.5,
        "position_size_pct": 2.0,
        "max_daily_loss_pct": 5.0
    }',
    true
);
