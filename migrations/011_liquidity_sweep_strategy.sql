-- Add Liquidity Sweep strategy type
-- BTC reversal strategy trading stop hunts at swing highs/lows

-- Insert the liquidity sweep strategy with default parameters for BTC
INSERT INTO strategies (id, user_id, name, strategy_type, params, is_active, risk)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'BTC Liquidity Sweep',
    'liquidity_sweep',
    '{
        "symbol": "BTC_USD",
        "timeframe": "H1",
        "swing_lookback": 20,
        "sweep_window": 3,
        "rsi_period": 14,
        "rsi_overbought": 70,
        "rsi_oversold": 30,
        "atr_period": 14,
        "atr_sl_mult": 1.0,
        "atr_tp_mult": 4.0,
        "use_trend_filter": true,
        "trend_ema": 200
    }',
    false,
    '{
        "stop_loss_pct": 2.0,
        "take_profit_pct": 8.0,
        "position_size_pct": 1.0,
        "max_daily_loss_pct": 5.0
    }'
) ON CONFLICT (name) DO UPDATE SET
    strategy_type = EXCLUDED.strategy_type,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;
