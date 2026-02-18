-- ═══════════════════════════════════════════════════════════════════════════════
-- Migration 031: Gold Optimized Bot Fleet (5 bots)
-- 
-- Based on aureum-optimizer-Gold-2026-02-15.json backtest results
-- All bots use D1 200 EMA direction filter (LONG/SHORT from instrument card)
-- ═══════════════════════════════════════════════════════════════════════════════

-- System user
-- a0000000-0000-0000-0000-000000000001

-- ═══════════════════════════════════════════════════════════════════════════════
-- 1. Gold Bollinger D1 — Score 97.5, WR 87.5%, Return 187%, Sharpe 21.38
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    'a1000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger D1',
    'bollinger_mean_reversion',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "atr_period": 14,
        "cooldown_seconds": 86400,
        "optimizer_sl_pips": 750,
        "optimizer_tp_pips": 1000,
        "optimizer_score": 97.5,
        "optimizer_win_rate": 87.5
    }',
    '{"stop_loss_pct": 2.0, "take_profit_pct": 3.0, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    true,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "GOLD_SUPER_BULL"],
        "volatility_pref": "normal",
        "min_adx": 0,
        "min_mrate_weight": 0.3
    }'
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

INSERT INTO bots (id, user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
VALUES (
    'b1000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger D1',
    'a1000000-0000-0000-0001-000000000001',
    0.3, 1, true, false, 750, 1000
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    override_sl_pips = EXCLUDED.override_sl_pips,
    override_tp_pips = EXCLUDED.override_tp_pips,
    is_active = true;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 2. Gold Bollinger H4 — Score 97.04, WR 85.19%, Return 75.5%, Sharpe 15.63
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    'a1000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger H4',
    'bollinger_mean_reversion',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H4",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "atr_period": 14,
        "cooldown_seconds": 14400,
        "optimizer_sl_pips": 400,
        "optimizer_tp_pips": 400,
        "optimizer_score": 97.04,
        "optimizer_win_rate": 85.19
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 1.5, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    true,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "GOLD_SUPER_BULL"],
        "volatility_pref": "normal",
        "min_adx": 0,
        "min_mrate_weight": 0.3
    }'
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

INSERT INTO bots (id, user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
VALUES (
    'b1000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger H4',
    'a1000000-0000-0000-0002-000000000001',
    0.3, 1, true, false, 400, 400
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    override_sl_pips = EXCLUDED.override_sl_pips,
    override_tp_pips = EXCLUDED.override_tp_pips,
    is_active = true;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 3. Gold Bollinger H1 — Score 96.84, WR 84.21%, Return 112.68%, Sharpe 19.86
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    'a1000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger H1',
    'bollinger_mean_reversion',
    '{
        "symbol": "XAU_USD",
        "timeframe": "H1",
        "bb_period": 20,
        "bb_std": 2.0,
        "rsi_period": 14,
        "atr_period": 14,
        "cooldown_seconds": 3600,
        "optimizer_sl_pips": 500,
        "optimizer_tp_pips": 800,
        "optimizer_score": 96.84,
        "optimizer_win_rate": 84.21
    }',
    '{"stop_loss_pct": 1.5, "take_profit_pct": 2.5, "position_size_pct": 2.0, "max_daily_loss_pct": 5.0}',
    true,
    'mean_reversion',
    '{
        "preferred_regimes": ["CHOPPY", "TREND", "GOLD_SUPER_BULL"],
        "volatility_pref": "normal",
        "min_adx": 0,
        "min_mrate_weight": 0.3
    }'
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

INSERT INTO bots (id, user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
VALUES (
    'b1000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Bollinger H1',
    'a1000000-0000-0000-0003-000000000001',
    0.3, 1, true, false, 500, 800
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    override_sl_pips = EXCLUDED.override_sl_pips,
    override_tp_pips = EXCLUDED.override_tp_pips,
    is_active = true;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 4. Gold London Breakout D1 — Score 96.47, WR 82.35%, Return 274.71%, Sharpe 13.46
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    'a1000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold London Breakout D1',
    'london_breakout',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "breakout_lookback": 20,
        "atr_period": 14,
        "cooldown_seconds": 86400,
        "optimizer_sl_pips": 2500,
        "optimizer_tp_pips": 2500,
        "optimizer_score": 96.47,
        "optimizer_win_rate": 82.35
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    true,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "volatility_pref": "high",
        "min_adx": 20,
        "min_mrate_weight": 0.4
    }'
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

INSERT INTO bots (id, user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
VALUES (
    'b1000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold London Breakout D1',
    'a1000000-0000-0000-0004-000000000001',
    0.2, 1, true, false, 2500, 2500
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    override_sl_pips = EXCLUDED.override_sl_pips,
    override_tp_pips = EXCLUDED.override_tp_pips,
    is_active = true;

-- ═══════════════════════════════════════════════════════════════════════════════
-- 5. Gold Donchian Breakout D1 — Score 95.29, WR 76.47%, Return 224.71%, Sharpe 9.89
-- ═══════════════════════════════════════════════════════════════════════════════
INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, is_active, mrate_category, profile)
VALUES (
    'a1000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Donchian Breakout D1',
    'donchian_breakout',
    '{
        "symbol": "XAU_USD",
        "timeframe": "D",
        "donchian_period": 20,
        "atr_period": 14,
        "cooldown_seconds": 86400,
        "optimizer_sl_pips": 2500,
        "optimizer_tp_pips": 2500,
        "optimizer_score": 95.29,
        "optimizer_win_rate": 76.47
    }',
    '{"stop_loss_pct": 3.0, "take_profit_pct": 3.0, "position_size_pct": 1.5, "max_daily_loss_pct": 5.0}',
    true,
    'breakout',
    '{
        "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
        "volatility_pref": "high",
        "min_adx": 20,
        "min_mrate_weight": 0.4
    }'
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    params = EXCLUDED.params,
    risk = EXCLUDED.risk;

INSERT INTO bots (id, user_id, name, strategy_id, lot_size, max_positions, is_active, auto_disabled, override_sl_pips, override_tp_pips)
VALUES (
    'b1000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    'Gold Donchian Breakout D1',
    'a1000000-0000-0000-0005-000000000001',
    0.2, 1, true, false, 2500, 2500
) ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    override_sl_pips = EXCLUDED.override_sl_pips,
    override_tp_pips = EXCLUDED.override_tp_pips,
    is_active = true;
