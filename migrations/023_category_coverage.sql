-- ═══════════════════════════════════════════════════════════════════════════════
-- Migration 023: Category Coverage Expansion
-- 
-- Fills MRATE orchestrator gaps with non-bollinger strategies.
-- Adds 13 bots: 6× trend, 3× breakout, 1× liquidity_sweep, 3× high-value adaptive.
-- All bots start inactive (auto_disabled=true) — MRATE orchestrator gates them.
--
-- Source: aureum-optimizer-ALL-2026-02-15.json (2000 candles, 8 instruments)
-- Selection: best_per_strategy with composite_score ≥ 85
-- ═══════════════════════════════════════════════════════════════════════════════

-- System user: a0000000-0000-0000-0000-000000000001

-- ═══════════════════════════════════════════════════════════════════════════════
-- TREND CATEGORY (6 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 1. ADX Trend - USD_JPY M15 — Score 96.36, WR 81.8%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY ADX M15',
    'adx_trend',
    '{
        "symbol": "USD_JPY",
        "timeframe": "M15",
        "adx_period": 14,
        "adx_threshold": 25,
        "ema_fast": 12,
        "ema_slow": 26,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.0,
        "cooldown_seconds": 180,
        "optimizer_sl_pips": 120,
        "optimizer_tp_pips": 160,
        "optimizer_score": 96.36,
        "optimizer_win_rate": 81.82
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 2.5, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND"],
        "best_sessions": ["ASIAN", "LONDON", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 12},
        "volatility_pref": "normal",
        "min_adx": 25,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY ADX M15', s.id, 0.3, 2, false, true, 120, 160
FROM strategies s WHERE s.name = 'OPT JPY ADX M15'
ON CONFLICT DO NOTHING;

-- 2. Triple Screen - USD_JPY M15 — Score 95.0, WR 75%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY Triple Screen M15',
    'triple_screen',
    '{
        "symbol": "USD_JPY",
        "timeframe": "M15",
        "ema_fast": 12,
        "ema_slow": 26,
        "rsi_period": 14,
        "macd_fast": 12,
        "macd_slow": 26,
        "macd_signal": 9,
        "atr_period": 14,
        "atr_sl_mult": 1.0,
        "atr_tp_mult": 1.5,
        "cooldown_seconds": 180,
        "optimizer_sl_pips": 32,
        "optimizer_tp_pips": 40,
        "optimizer_score": 95.0,
        "optimizer_win_rate": 75.0
    }',
    '{"stop_loss_pct": 0.5, "take_profit_pct": 1.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["ASIAN", "LONDON", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 12},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY Triple M15', s.id, 0.3, 2, false, true, 32, 40
FROM strategies s WHERE s.name = 'OPT JPY Triple Screen M15'
ON CONFLICT DO NOTHING;

-- 3. MACD Divergence - USD_JPY D — Score 93.5, WR 75%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY MACD Div D',
    'macd_divergence',
    '{
        "symbol": "USD_JPY",
        "timeframe": "D",
        "macd_fast": 12,
        "macd_slow": 26,
        "macd_signal": 9,
        "rsi_period": 14,
        "atr_period": 14,
        "atr_sl_mult": 2.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1000,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 93.5,
        "optimizer_win_rate": 75.0
    }',
    '{"stop_loss_pct": 2.5, "take_profit_pct": 5.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND"],
        "best_sessions": ["ASIAN", "LONDON", "NEW_YORK"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY MACD Div', s.id, 0.3, 1, false, true, 1000, 1000
FROM strategies s WHERE s.name = 'OPT JPY MACD Div D'
ON CONFLICT DO NOTHING;

-- 4. London NY Trend Continuation - XAU H1 — Score 91.88, WR 80%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold London NY H1',
    'london_ny_trend_continuation',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H1",
        "ema_fast": 20,
        "ema_slow": 50,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.0,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 150,
        "optimizer_tp_pips": 150,
        "optimizer_score": 91.88,
        "optimizer_win_rate": 80.0
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 7, "end": 21},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold London NY', s.id, 0.3, 2, false, true, 150, 150
FROM strategies s WHERE s.name = 'OPT Gold London NY H1'
ON CONFLICT DO NOTHING;

-- 5. EMA Ribbon - XAU D — Score 91.67, WR 58%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold EMA Ribbon D',
    'ema_ribbon',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "ema_8": 8,
        "ema_13": 13,
        "ema_21": 21,
        "ema_55": 55,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1500,
        "optimizer_tp_pips": 4000,
        "optimizer_score": 91.67,
        "optimizer_win_rate": 58.33
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 10.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold EMA Ribbon', s.id, 0.2, 1, false, true, 1500, 4000
FROM strategies s WHERE s.name = 'OPT Gold EMA Ribbon D'
ON CONFLICT DO NOTHING;

-- 6. SMA Crossover - USD_JPY D — Score 90.77, WR 54%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY SMA D',
    'sma_crossover',
    '{
        "symbol": "USD_JPY",
        "timeframe": "D",
        "fast_period": 10,
        "slow_period": 50,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 400,
        "optimizer_tp_pips": 1500,
        "optimizer_score": 90.77,
        "optimizer_win_rate": 53.85
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 6.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND"],
        "best_sessions": ["ASIAN", "LONDON", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY SMA D', s.id, 0.3, 1, false, true, 400, 1500
FROM strategies s WHERE s.name = 'OPT JPY SMA D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- BREAKOUT CATEGORY (3 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 7. London Breakout - XAU D — Score 94.29, WR 71%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold London BO D v2',
    'london_breakout',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "breakout_lookback": 20,
        "atr_period": 14,
        "atr_sl_mult": 3.0,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 2500,
        "optimizer_tp_pips": 4000,
        "optimizer_score": 94.29,
        "optimizer_win_rate": 71.43
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 10.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "OVERLAP"],
        "hours_utc": {"start": 7, "end": 17},
        "volatility_pref": "high",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold London BO v2', s.id, 0.2, 1, false, true, 2500, 4000
FROM strategies s WHERE s.name = 'OPT Gold London BO D v2'
ON CONFLICT DO NOTHING;

-- 8. Donchian Breakout - USD_JPY M15 — Score 91.73, WR 67%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY Donchian M15',
    'donchian_breakout',
    '{
        "symbol": "USD_JPY",
        "timeframe": "M15",
        "donchian_period": 20,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.5,
        "cooldown_seconds": 180,
        "optimizer_sl_pips": 120,
        "optimizer_tp_pips": 200,
        "optimizer_score": 91.73,
        "optimizer_win_rate": 66.67
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["ASIAN", "LONDON", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 12},
        "volatility_pref": "high",
        "min_adx": 20,
        "rsi_range": {"min": 35, "max": 65},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY Donchian M15', s.id, 0.3, 2, false, true, 120, 200
FROM strategies s WHERE s.name = 'OPT JPY Donchian M15'
ON CONFLICT DO NOTHING;

-- 9. Trendline Bounce - WTICO D — Score 94.0, WR 70%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil Trendline D',
    'trendline_bounce',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "D",
        "lookback_period": 50,
        "pivot_strength": 3,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1500,
        "optimizer_tp_pips": 2500,
        "optimizer_score": 94.0,
        "optimizer_win_rate": 70.0
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 8.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "CHOPPY"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 15,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil Trendline', s.id, 0.5, 1, false, true, 1500, 2500
FROM strategies s WHERE s.name = 'OPT Oil Trendline D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- LIQUIDITY_SWEEP CATEGORY (1 bot - critical gap filler)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 10. Liquidity Sweep - GBP D — Score 87.99, WR 40%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT GBP Liquidity Sweep D',
    'liquidity_sweep',
    '{
        "symbol": "GBP_USD",
        "timeframe": "D",
        "lookback_period": 20,
        "wick_threshold_pct": 0.5,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 400,
        "optimizer_tp_pips": 2000,
        "optimizer_score": 87.99,
        "optimizer_win_rate": 40.0
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 8.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'liquidity_sweep',
    '{
        "preferred_regimes": ["TREND", "CHOPPY"],
        "best_sessions": ["LONDON", "NEW_YORK"],
        "hours_utc": {"start": 7, "end": 20},
        "volatility_pref": "normal",
        "min_adx": 15,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT GBP Liquidity Sweep', s.id, 0.5, 1, false, true, 400, 2000
FROM strategies s WHERE s.name = 'OPT GBP Liquidity Sweep D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- HIGH-VALUE ADDITIONAL (3 bots - adaptive/multi-category)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 11. Forecast Confidence - WTICO D — Score 93.33, WR 67%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil Forecast D',
    'forecast_confidence',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "D",
        "lookback_period": 30,
        "confidence_threshold": 0.7,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1500,
        "optimizer_tp_pips": 2000,
        "optimizer_score": 93.33,
        "optimizer_win_rate": 66.67
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 6.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "CHOPPY"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.3,
        "adaptive": true
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil Forecast', s.id, 0.5, 1, false, true, 1500, 2000
FROM strategies s WHERE s.name = 'OPT Oil Forecast D'
ON CONFLICT DO NOTHING;

-- 12. Stochastic Crossover - WTICO H1 — Score 93.25, WR 82%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil Stochastic H1',
    'stochastic_crossover',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "H1",
        "k_period": 14,
        "d_period": 3,
        "smooth": 3,
        "oversold": 20,
        "overbought": 80,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.0,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 300,
        "optimizer_tp_pips": 300,
        "optimizer_score": 93.25,
        "optimizer_win_rate": 81.82
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 20, "max": 80},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil Stochastic', s.id, 0.5, 2, false, true, 300, 300
FROM strategies s WHERE s.name = 'OPT Oil Stochastic H1'
ON CONFLICT DO NOTHING;

-- 13. MACD Crossover - WTICO H1 — Score 85.71, WR 45%
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil MACD H1',
    'macd_crossover',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "H1",
        "macd_fast": 12,
        "macd_slow": 26,
        "macd_signal": 9,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 100,
        "optimizer_tp_pips": 500,
        "optimizer_score": 85.71,
        "optimizer_win_rate": 45.45
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 5.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND"],
        "best_sessions": ["LONDON", "NEW_YORK"],
        "hours_utc": {"start": 8, "end": 20},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 35, "max": 65},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil MACD H1', s.id, 0.5, 2, false, true, 100, 500
FROM strategies s WHERE s.name = 'OPT Oil MACD H1'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- Summary: 13 category-balanced bots added
-- ═══════════════════════════════════════════════════════════════════════════════
-- Total fleet: 13 (existing) + 13 (new) = 26 bots
--
-- MRATE Category Distribution (new fleet):
-- - Mean Reversion:    10 bots (9 bollinger + 1 stochastic)
-- - Trend:             10 bots (3 old + 7 new: ADX, Triple, MACD Div, London NY, EMA, SMA, MACD XO)
-- - Breakout:           4 bots (1 old + 3 new: London BO v2, Donchian, Trendline)
-- - Liquidity Sweep:    1 bot (new: GBP)
-- - Adaptive/Multi:     1 bot (Forecast Confidence)
--
-- Bot List (new):
-- 1. OPT JPY ADX M15         | USD_JPY | M15 | trend          | 96.36
-- 2. OPT JPY Triple M15      | USD_JPY | M15 | trend          | 95.00
-- 3. OPT JPY MACD Div        | USD_JPY | D   | trend          | 93.50
-- 4. OPT Gold London NY      | XAU_USD | H1  | trend          | 91.88
-- 5. OPT Gold EMA Ribbon     | XAU_USD | D   | trend          | 91.67
-- 6. OPT JPY SMA D           | USD_JPY | D   | trend          | 90.77
-- 7. OPT Gold London BO v2   | XAU_USD | D   | breakout       | 94.29
-- 8. OPT JPY Donchian M15    | USD_JPY | M15 | breakout       | 91.73
-- 9. OPT Oil Trendline       | WTICO   | D   | breakout       | 94.00
-- 10. OPT GBP Liquidity Sweep | GBP_USD | D   | liquidity_sweep| 87.99
-- 11. OPT Oil Forecast        | WTICO   | D   | trend/adaptive | 93.33
-- 12. OPT Oil Stochastic      | WTICO   | H1  | mean_reversion | 93.25
-- 13. OPT Oil MACD H1         | WTICO   | H1  | trend          | 85.71
--
-- All bots: is_active=false, auto_disabled=true → orchestrator controls activation
