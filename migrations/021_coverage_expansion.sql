-- ═══════════════════════════════════════════════════════════════════════════════
-- Migration 021: Coverage Expansion
-- Fills gaps: PANIC regime, Asian session, BTC depth, Oil activation,
-- FX H4/D swing, Gold H4/D swing, risk-off strategies, index trading
-- ═══════════════════════════════════════════════════════════════════════════════

-- Helper: system user
-- All strategies owned by system user a0000000-...-0001

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. PANIC REGIME — Gold safe-haven bot (long-only gold in panic)
-- ═══════════════════════════════════════════════════════════════════════════════

INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Panic Safe Haven H4',
    'real_yield_momentum',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H4",
        "trend_ema": 200,
        "value_ema": 21,
        "rsi_period": 14,
        "rsi_oversold": 25,
        "rsi_overbought": 80,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 10.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["PANIC", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP", "ASIAN"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "high",
        "min_adx": 0,
        "rsi_range": {"min": 0, "max": 100},
        "min_mrate_weight": 0.3,
        "adaptive": true
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

-- Create bot for it
INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'Gold Panic Haven', s.id, 0.5, 2, false, true
FROM strategies s WHERE s.name = 'Gold Panic Safe Haven H4'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. ASIAN SESSION — Gold and JPY mean reversion during Asian hours
-- ═══════════════════════════════════════════════════════════════════════════════

-- Gold Asian RSI Reversion
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Asian RSI Reversion H1',
    'rsi_reversion_2',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H1",
        "rsi_period": 14,
        "rsi_oversold": 28,
        "rsi_overbought": 72,
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 180
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 4.5, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "PANIC"],
        "best_sessions": ["ASIAN"],
        "hours_utc": {"start": 0, "end": 8},
        "volatility_pref": "low",
        "min_adx": 0,
        "rsi_range": {"min": 0, "max": 30},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'Gold Asian RSI', s.id, 0.3, 2, false, true
FROM strategies s WHERE s.name = 'Gold Asian RSI Reversion H1'
ON CONFLICT DO NOTHING;

-- JPY Asian Bollinger Reversion
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'JPY Asian Bollinger H1',
    'bollinger_mean_reversion',
    '{
        "symbol": "USD_JPY",
        "timeframe": "H1",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.5,
        "cooldown_seconds": 180
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 2.5, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "PANIC"],
        "best_sessions": ["ASIAN"],
        "hours_utc": {"start": 0, "end": 8},
        "volatility_pref": "low",
        "min_adx": 0,
        "rsi_range": {"min": 25, "max": 75},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'JPY Asian Bollinger', s.id, 0.1, 2, false, true
FROM strategies s WHERE s.name = 'JPY Asian Bollinger H1'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. BTC DEPTH — More BTC bots + activate Macro BTC Momentum
-- ═══════════════════════════════════════════════════════════════════════════════

-- Activate existing Macro BTC Momentum bot
UPDATE bots SET is_active = true WHERE name = 'Macro BTC Momentum';

-- BTC RSI Reversion H1
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'BTC RSI Reversion H1',
    'rsi_reversion_2',
    '{
        "symbol": "BTC_USD",
        "timeframe": "H1",
        "rsi_period": 14,
        "rsi_oversold": 25,
        "rsi_overbought": 75,
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 300
    }',
    '{"stop_loss_pct": 2.5, "take_profit_pct": 7.5, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "PANIC"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 0,
        "rsi_range": {"min": 0, "max": 30},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'BTC RSI Reversion', s.id, 1, 2, true, false
FROM strategies s WHERE s.name = 'BTC RSI Reversion H1'
ON CONFLICT DO NOTHING;

-- BTC Donchian Breakout H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'BTC Donchian Breakout H4',
    'donchian_breakout',
    '{
        "symbol": "BTC_USD",
        "timeframe": "H4",
        "donchian_period": 20,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 9.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 7, "end": 20},
        "volatility_pref": "normal",
        "min_adx": 15,
        "rsi_range": {"min": 0, "max": 100},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'BTC Breakout H4', s.id, 1, 2, true, false
FROM strategies s WHERE s.name = 'BTC Donchian Breakout H4'
ON CONFLICT DO NOTHING;

-- BTC EMA Ribbon D (swing)
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'BTC EMA Ribbon Daily',
    'ema_ribbon',
    '{
        "symbol": "BTC_USD",
        "timeframe": "D",
        "atr_period": 14,
        "atr_sl_mult": 2.5,
        "atr_tp_mult": 5.0,
        "cooldown_seconds": 3600
    }',
    '{"stop_loss_pct": 4.0, "take_profit_pct": 12.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP", "ASIAN"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "any",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'BTC Daily Swing', s.id, 1, 1, true, false
FROM strategies s WHERE s.name = 'BTC EMA Ribbon Daily'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. OIL — Activate inactive bots
-- ═══════════════════════════════════════════════════════════════════════════════

UPDATE bots SET is_active = true WHERE name IN ('Oil MACD H1', 'Oil Breakout');

-- ═══════════════════════════════════════════════════════════════════════════════
-- 5. FX H4/D SWING — EUR/USD and USD/JPY higher timeframes
-- ═══════════════════════════════════════════════════════════════════════════════

-- EUR/USD MACD H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'EUR MACD Crossover H4',
    'macd_crossover',
    '{
        "symbol": "EUR_USD",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND"],
        "best_sessions": ["LONDON", "NEW_YORK"],
        "hours_utc": {"start": 8, "end": 20},
        "volatility_pref": "normal",
        "min_adx": 18,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'EUR MACD Swing H4', s.id, 0.2, 2, true, false
FROM strategies s WHERE s.name = 'EUR MACD Crossover H4'
ON CONFLICT DO NOTHING;

-- EUR/USD Triple Screen Daily
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'EUR Triple Screen Daily',
    'triple_screen',
    '{
        "symbol": "EUR_USD",
        "timeframe": "D",
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 3600
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 4.5, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "best_sessions": ["LONDON", "NEW_YORK"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'EUR Daily Swing', s.id, 0.15, 1, true, false
FROM strategies s WHERE s.name = 'EUR Triple Screen Daily'
ON CONFLICT DO NOTHING;

-- USD/JPY EMA Ribbon H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'JPY EMA Ribbon H4',
    'ema_ribbon',
    '{
        "symbol": "USD_JPY",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "PANIC"],
        "best_sessions": ["LONDON", "NEW_YORK", "ASIAN"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "normal",
        "min_adx": 18,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'JPY EMA Swing H4', s.id, 0.1, 2, true, false
FROM strategies s WHERE s.name = 'JPY EMA Ribbon H4'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 7. GOLD H4/D SWING — Reduce M5/M15 concentration
-- ═══════════════════════════════════════════════════════════════════════════════

-- Gold Macro Swing H4 (activate existing inactive bot)
UPDATE bots SET is_active = true WHERE name = 'Macro Gold Momentum';

-- Gold Triple Screen H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Triple Screen H4',
    'triple_screen',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.5,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 5.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
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
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'Gold Triple H4', s.id, 0.3, 2, true, false
FROM strategies s WHERE s.name = 'Gold Triple Screen H4'
ON CONFLICT DO NOTHING;

-- Gold Trendline Daily Swing
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Trendline Daily',
    'trendline_bounce',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 3600
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 8.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL", "PANIC"],
        "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP", "ASIAN"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "any",
        "min_adx": 15,
        "rsi_range": {"min": 30, "max": 70},
        "min_mrate_weight": 0.4,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'Gold Daily Trendline', s.id, 0.2, 1, true, false
FROM strategies s WHERE s.name = 'Gold Trendline Daily'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 8. RISK-OFF — Long gold + Long JPY when risk is elevated
-- ═══════════════════════════════════════════════════════════════════════════════

-- Gold DXY Divergence as risk-off play (already implemented, needs a bot)
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'Gold Risk-Off DXY Divergence H4',
    'gold_dxy_divergence',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H4",
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.5,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 5.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["PANIC", "GOLD_SUPER_BULL", "CHOPPY"],
        "best_sessions": ["LONDON", "NEW_YORK"],
        "hours_utc": {"start": 8, "end": 20},
        "volatility_pref": "high",
        "min_adx": 15,
        "rsi_range": {"min": 0, "max": 100},
        "min_mrate_weight": 0.3,
        "adaptive": true
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'Gold Risk-Off', s.id, 0.3, 2, false, true
FROM strategies s WHERE s.name = 'Gold Risk-Off DXY Divergence H4'
ON CONFLICT DO NOTHING;

-- JPY Risk-Off Momentum (JPY strengthens in risk-off: short USD/JPY)
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'JPY Risk-Off MACD H4',
    'macd_crossover',
    '{
        "symbol": "USD_JPY",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["PANIC", "CHOPPY"],
        "best_sessions": ["LONDON", "NEW_YORK", "ASIAN"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "high",
        "min_adx": 15,
        "rsi_range": {"min": 0, "max": 100},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'JPY Risk-Off', s.id, 0.1, 2, false, true
FROM strategies s WHERE s.name = 'JPY Risk-Off MACD H4'
ON CONFLICT DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 9. INDICES — SPX500 and NAS100 for equity diversification
-- ═══════════════════════════════════════════════════════════════════════════════

-- S&P 500 EMA Ribbon H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'SPX500 EMA Ribbon H4',
    'ema_ribbon',
    '{
        "symbol": "SPX500_USD",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.0, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 13, "end": 21},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 35, "max": 65},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'SPX500 Trend H4', s.id, 0.01, 2, true, false
FROM strategies s WHERE s.name = 'SPX500 EMA Ribbon H4'
ON CONFLICT DO NOTHING;

-- S&P 500 RSI Mean Reversion H1
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'SPX500 RSI Reversion H1',
    'rsi_reversion_2',
    '{
        "symbol": "SPX500_USD",
        "timeframe": "H1",
        "rsi_period": 14,
        "rsi_oversold": 30,
        "rsi_overbought": 70,
        "trend_ema": 200,
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 2.5,
        "cooldown_seconds": 300
    }',
    '{"stop_loss_pct": 0.8, "take_profit_pct": 2.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND"],
        "best_sessions": ["NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 13, "end": 21},
        "volatility_pref": "low",
        "min_adx": 0,
        "rsi_range": {"min": 0, "max": 30},
        "min_mrate_weight": 0.3,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'SPX500 RSI Bounce', s.id, 0.01, 2, true, false
FROM strategies s WHERE s.name = 'SPX500 RSI Reversion H1'
ON CONFLICT DO NOTHING;

-- NAS100 MACD Trend H4
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'NAS100 MACD Trend H4',
    'macd_crossover',
    '{
        "symbol": "NAS100_USD",
        "timeframe": "H4",
        "atr_period": 14,
        "atr_sl_mult": 1.5,
        "atr_tp_mult": 3.0,
        "cooldown_seconds": 600
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 4.5, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'trend',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["NEW_YORK", "OVERLAP"],
        "hours_utc": {"start": 13, "end": 21},
        "volatility_pref": "normal",
        "min_adx": 20,
        "rsi_range": {"min": 35, "max": 65},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'NAS100 Trend H4', s.id, 0.01, 2, true, false
FROM strategies s WHERE s.name = 'NAS100 MACD Trend H4'
ON CONFLICT DO NOTHING;

-- NAS100 Donchian Breakout Daily
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    gen_random_uuid(),
    'a0000000-0000-0000-0000-000000000001',
    'NAS100 Donchian Breakout D',
    'donchian_breakout',
    '{
        "symbol": "NAS100_USD",
        "timeframe": "D",
        "donchian_period": 20,
        "atr_period": 14,
        "atr_sl_mult": 2.0,
        "atr_tp_mult": 4.0,
        "cooldown_seconds": 3600
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 6.0, "position_size_pct": 1.0, "max_daily_loss_pct": 5.0}',
    false,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
        "best_sessions": ["NEW_YORK"],
        "hours_utc": {"start": 0, "end": 24},
        "volatility_pref": "high",
        "min_adx": 15,
        "rsi_range": {"min": 0, "max": 100},
        "min_mrate_weight": 0.5,
        "adaptive": false
    }'
) ON CONFLICT (name) DO UPDATE SET params = EXCLUDED.params, profile = EXCLUDED.profile;

INSERT INTO bots (user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled)
SELECT 'a0000000-0000-0000-0000-000000000001', 'NAS100 Breakout D', s.id, 0.01, 1, true, false
FROM strategies s WHERE s.name = 'NAS100 Donchian Breakout D'
ON CONFLICT DO NOTHING;
