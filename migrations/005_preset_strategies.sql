-- Add well-known trading strategies as presets
-- These are battle-tested strategies with proper risk management
-- Uses existing trader user

-- 1. MACD Crossover Strategy
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'MACD Crossover',
    'macd_crossover',
    '{"fast_period": 12, "slow_period": 26, "signal_period": 9, "sl_pips": 25, "tp_pips": 50}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.04, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 2. Donchian Channel Breakout (Turtle Trading)
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000002',
    'a0000000-0000-0000-0000-000000000001',
    'Donchian Breakout',
    'donchian_breakout',
    '{"channel_period": 20, "exit_period": 10, "sl_pips": 30, "tp_pips": 60}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.04, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 3. ADX Trend Strategy
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000003',
    'a0000000-0000-0000-0000-000000000001',
    'ADX Trend Filter',
    'adx_trend',
    '{"adx_period": 14, "adx_threshold": 25, "sl_pips": 30, "tp_pips": 45}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.03, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 4. RSI Mean Reversion
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000004',
    'a0000000-0000-0000-0000-000000000001',
    'RSI Mean Reversion',
    'rsi_mean_reversion',
    '{"rsi_period": 14, "oversold": 30, "overbought": 70, "sl_pips": 20, "tp_pips": 30}',
    '{"stop_loss_pct": 0.015, "take_profit_pct": 0.025, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 5. Bollinger Band Mean Reversion
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000005',
    'a0000000-0000-0000-0000-000000000001',
    'Bollinger Mean Reversion',
    'bollinger_mean_reversion',
    '{"bb_period": 20, "bb_std": 2.0, "sl_pips": 15, "tp_pips": 25}',
    '{"stop_loss_pct": 0.012, "take_profit_pct": 0.02, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 6. MACD Histogram Divergence
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000006',
    'a0000000-0000-0000-0000-000000000001',
    'MACD Divergence',
    'macd_divergence',
    '{"fast_period": 12, "slow_period": 26, "signal_period": 9, "lookback": 10, "sl_pips": 25, "tp_pips": 50}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.04, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 7. Stochastic Crossover
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000007',
    'a0000000-0000-0000-0000-000000000001',
    'Stochastic Crossover',
    'stochastic_crossover',
    '{"k_period": 14, "d_period": 3, "oversold": 20, "overbought": 80, "sl_pips": 20, "tp_pips": 35}',
    '{"stop_loss_pct": 0.015, "take_profit_pct": 0.028, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 8. London Session Breakout
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000008',
    'a0000000-0000-0000-0000-000000000001',
    'London Breakout',
    'london_breakout',
    '{"range_period": 12, "sl_pips": 15, "tp_pips": 30}',
    '{"stop_loss_pct": 0.012, "take_profit_pct": 0.024, "max_positions": 2}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 9. EMA Ribbon Trend
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000009',
    'a0000000-0000-0000-0000-000000000001',
    'EMA Ribbon',
    'ema_ribbon',
    '{"ema_periods": [8, 13, 21, 34, 55], "sl_pips": 25, "tp_pips": 50}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.04, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;

-- 10. Triple Screen (Elder)
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, created_at)
VALUES (
    'b1000000-0000-0000-0000-000000000010',
    'a0000000-0000-0000-0000-000000000001',
    'Triple Screen',
    'triple_screen',
    '{"trend_ema": 13, "stoch_k": 5, "stoch_d": 3, "sl_pips": 30, "tp_pips": 60}',
    '{"stop_loss_pct": 0.02, "take_profit_pct": 0.04, "max_positions": 3}',
    NOW()
) ON CONFLICT (id) DO NOTHING;
