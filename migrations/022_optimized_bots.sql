-- ═══════════════════════════════════════════════════════════════════════════════
-- Migration 022: Optimizer-Derived Bot Fleet
-- 
-- Replaces manual bot configs with backtest-optimized parameters.
-- 13 bots across 7 instruments, selected from 1000-candle optimizer runs.
-- Selection criteria: composite_score ≥ 85, win_rate ≥ 50%, diverse strategies.
-- All bots start inactive (auto_disabled=true) — MRATE orchestrator gates them.
--
-- Source: /optemised/aureum-optimizer-*-2026-02-15.json
-- Note: SPX500_USD not included (export was duplicate of NAS100_USD)
-- ═══════════════════════════════════════════════════════════════════════════════

-- System user
-- a0000000-0000-0000-0000-000000000001

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. XAU_USD — Gold (3 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 1a. Gold Bollinger Daily — Score 97.5, WR 87.5%, Ret 187%, Sharpe 21.4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold Bollinger D',
    'bollinger_mean_reversion',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 750,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 97.5,
        "optimizer_win_rate": 87.5
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 6.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold Bollinger', s.id, 0.3, 1, false, true, 750, 1000
FROM strategies s WHERE s.name = 'OPT Gold Bollinger D'
ON CONFLICT DO NOTHING;

-- 1b. Gold London Breakout Daily — Score 96.47, WR 82.4%, Ret 275%, Sharpe 13.5
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold London Breakout D',
    'london_breakout',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "breakout_lookback": 20,
        "atr_period": 14,
        "atr_sl_mult": 2.5,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 2500,
        "optimizer_tp_pips": 2500,
        "optimizer_score": 96.47,
        "optimizer_win_rate": 82.35
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 8.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
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
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold London BO', s.id, 0.2, 1, false, true, 2500, 2500
FROM strategies s WHERE s.name = 'OPT Gold London Breakout D'
ON CONFLICT DO NOTHING;

-- 1c. Gold ADX Trend Daily — Score 92.0, WR 60%, Ret 346%, Sharpe 10.3
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Gold ADX Trend D',
    'adx_trend',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "adx_period": 14,
        "adx_threshold": 25,
        "ema_fast": 12,
        "ema_slow": 26,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1500,
        "optimizer_tp_pips": 4000,
        "optimizer_score": 92.0,
        "optimizer_win_rate": 60.0
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 10.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 25,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Gold ADX Trend', s.id, 0.2, 1, false, true, 1500, 4000
FROM strategies s WHERE s.name = 'OPT Gold ADX Trend D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. BTC_USD — Bitcoin (2 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 2a. BTC Triple Screen H4 — Score 91.75, WR 83.3%, Sharpe 14.0
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT BTC Triple Screen H4',
    'triple_screen',
    '{
        "symbol": "BTC_USD",
        "timeframe": "H4",
        "ema_fast": 12,
        "ema_slow": 26,
        "rsi_period": 14,
        "macd_fast": 12,
        "macd_slow": 26,
        "macd_signal": 9,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1000,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 91.75,
        "optimizer_win_rate": 83.33
    }',
    '{"stop_loss_pct": 2.5, "take_profit_pct": 5.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT BTC Triple Screen', s.id, 1, 1, false, true, 1000, 1000
FROM strategies s WHERE s.name = 'OPT BTC Triple Screen H4'
ON CONFLICT DO NOTHING;

-- 2b. BTC Bollinger Daily — Score 91.11, WR 78.6%, Sharpe 11.0
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT BTC Bollinger D',
    'bollinger_mean_reversion',
    '{
        "symbol": "BTC_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 2500,
        "optimizer_tp_pips": 2500,
        "optimizer_score": 91.11,
        "optimizer_win_rate": 78.57
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 6.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT BTC Bollinger', s.id, 1, 1, false, true, 2500, 2500
FROM strategies s WHERE s.name = 'OPT BTC Bollinger D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. EUR_USD — Euro (1 bot)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 3a. EUR Bollinger Daily — Score 90.2, WR 63.6%, Ret 24.7%, Sharpe 9.9
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT EUR Bollinger D',
    'bollinger_mean_reversion',
    '{
        "symbol": "EUR_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 250,
        "optimizer_tp_pips": 500,
        "optimizer_score": 90.2,
        "optimizer_win_rate": 63.64
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 7, "end": 21},
        "volatility_pref": "low",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT EUR Bollinger', s.id, 0.5, 1, false, true, 250, 500
FROM strategies s WHERE s.name = 'OPT EUR Bollinger D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. GBP_USD — Sterling (2 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 4a. GBP Bollinger Daily — Score 96.98, WR 90.0%, Ret 39.8%, Sharpe 21.0
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT GBP Bollinger D',
    'bollinger_mean_reversion',
    '{
        "symbol": "GBP_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 500,
        "optimizer_tp_pips": 500,
        "optimizer_score": 96.98,
        "optimizer_win_rate": 90.0
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 4.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 7, "end": 21},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT GBP Bollinger', s.id, 0.5, 1, false, true, 500, 500
FROM strategies s WHERE s.name = 'OPT GBP Bollinger D'
ON CONFLICT DO NOTHING;

-- 4b. GBP MACD Crossover Daily — Score 91.07, WR 63.6%, Ret 33.4%, Sharpe 8.9
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT GBP MACD D',
    'macd_crossover',
    '{
        "symbol": "GBP_USD",
        "timeframe": "D",
        "macd_fast": 12,
        "macd_slow": 26,
        "macd_signal": 9,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 400,
        "optimizer_tp_pips": 750,
        "optimizer_score": 91.07,
        "optimizer_win_rate": 63.64
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 5.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
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
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT GBP MACD', s.id, 0.5, 1, false, true, 400, 750
FROM strategies s WHERE s.name = 'OPT GBP MACD D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 5. USD_JPY — Yen (2 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 5a. JPY Bollinger H4 — Score 98.57, WR 92.9%, Sharpe 36.3
-- Note: returns inflated by JPY pip mechanics. Conservative lot size.
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY Bollinger H4',
    'bollinger_mean_reversion',
    '{
        "symbol": "USD_JPY",
        "timeframe": "H4",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.5,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 100,
        "optimizer_tp_pips": 200,
        "optimizer_score": 98.57,
        "optimizer_win_rate": 92.86
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 2.5, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["ASIAN", "LONDON", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY Bollinger', s.id, 0.3, 1, false, true, 100, 200
FROM strategies s WHERE s.name = 'OPT JPY Bollinger H4'
ON CONFLICT DO NOTHING;

-- 5b. JPY SMA Crossover H4 — Score 96.84, WR 84.2%, Sharpe 17.1
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT JPY SMA H4',
    'sma_crossover',
    '{
        "symbol": "USD_JPY",
        "timeframe": "H4",
        "fast_period": 10,
        "slow_period": 50,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.0,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 160,
        "optimizer_tp_pips": 200,
        "optimizer_score": 96.84,
        "optimizer_win_rate": 84.21
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
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
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT JPY SMA', s.id, 0.3, 1, false, true, 160, 200
FROM strategies s WHERE s.name = 'OPT JPY SMA H4'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 6. WTICO_USD — Oil (2 bots)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 6a. Oil Bollinger Daily — Score 98.67, WR 93.3%, Ret 130%, Sharpe 38.2
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil Bollinger D',
    'bollinger_mean_reversion',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 400,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 98.67,
        "optimizer_win_rate": 93.33
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 6.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil Bollinger', s.id, 0.5, 1, false, true, 400, 1000
FROM strategies s WHERE s.name = 'OPT Oil Bollinger D'
ON CONFLICT DO NOTHING;

-- 6b. Oil RSI Mean Reversion Daily — Score 98.18, WR 90.9%, Ret 82%, Sharpe 20.1
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT Oil RSI D',
    'rsi_mean_reversion',
    '{
        "symbol": "WTICO_USD",
        "timeframe": "D",
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 2.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600,
        "optimizer_sl_pips": 1000,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 98.18,
        "optimizer_win_rate": 90.91
    }',
    '{"stop_loss_pct": 2.5, "take_profit_pct": 5.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
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
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT Oil RSI', s.id, 0.5, 1, false, true, 1000, 1000
FROM strategies s WHERE s.name = 'OPT Oil RSI D'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 7. NAS100_USD — NASDAQ (1 bot)
-- ═══════════════════════════════════════════════════════════════════════════════

-- 7a. NASDAQ Bollinger H1 — Score 85.06, WR 64.5%, Ret 9.5%, Sharpe 7.8
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'OPT NAS Bollinger H1',
    'bollinger_mean_reversion',
    '{
        "symbol": "NAS100_USD",
        "timeframe": "H1",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 300,
        "optimizer_sl_pips": 500,
        "optimizer_tp_pips": 800,
        "optimizer_score": 85.06,
        "optimizer_win_rate": 64.52
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 13, "end": 21},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
);

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
SELECT 'a0000000-0000-0000-0000-000000000001', 'OPT NAS Bollinger', s.id, 0.5, 1, false, true, 500, 800
FROM strategies s WHERE s.name = 'OPT NAS Bollinger H1'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- Summary: 13 optimizer-derived bots
-- ═══════════════════════════════════════════════════════════════════════════════
-- Bot Name              | Instrument | Strategy              | TF | SL   | TP   | Score
-- OPT Gold Bollinger    | XAU_USD    | bollinger_mean_rev    | D  | 750  | 1000 | 97.50
-- OPT Gold London BO    | XAU_USD    | london_breakout       | D  | 2500 | 2500 | 96.47
-- OPT Gold ADX Trend    | XAU_USD    | adx_trend             | D  | 1500 | 4000 | 92.00
-- OPT BTC Triple Screen | BTC_USD    | triple_screen         | H4 | 1000 | 1000 | 91.75
-- OPT BTC Bollinger     | BTC_USD    | bollinger_mean_rev    | D  | 2500 | 2500 | 91.11
-- OPT EUR Bollinger     | EUR_USD    | bollinger_mean_rev    | D  | 250  | 500  | 90.20
-- OPT GBP Bollinger     | GBP_USD    | bollinger_mean_rev    | D  | 500  | 500  | 96.98
-- OPT GBP MACD          | GBP_USD    | macd_crossover        | D  | 400  | 750  | 91.07
-- OPT JPY Bollinger     | USD_JPY    | bollinger_mean_rev    | H4 | 100  | 200  | 98.57
-- OPT JPY SMA           | USD_JPY    | sma_crossover         | H4 | 160  | 200  | 96.84
-- OPT Oil Bollinger     | WTICO_USD  | bollinger_mean_rev    | D  | 400  | 1000 | 98.67
-- OPT Oil RSI           | WTICO_USD  | rsi_mean_reversion    | D  | 1000 | 1000 | 98.18
-- OPT NAS Bollinger     | NAS100_USD | bollinger_mean_rev    | H1 | 500  | 800  | 85.06
--
-- Strategy mix: 7× bollinger_mean_reversion, 1× london_breakout, 1× adx_trend,
--               1× triple_screen, 1× macd_crossover, 1× sma_crossover, 1× rsi_mean_reversion
-- Regime mix:   9× mean_reversion, 3× trend, 1× breakout
-- Timeframes:   9× D, 3× H4, 1× H1
-- All bots: is_active=false, auto_disabled=true → orchestrator controls activation
