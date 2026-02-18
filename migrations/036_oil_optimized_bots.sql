-- Migration: Create optimized WTI Oil bots based on backtest results
-- Date: 2026-02-18
-- Results from: aureum-optimizer-OIL-2026-02-15.json

-- =====================================================
-- STRATEGIES
-- =====================================================

-- 1. Oil Bollinger D1 (93.3% WR, Score 98.67)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a3000000-0000-0000-0001-000000000001',
    'Oil Bollinger D1',
    'bollinger_mean_reversion',
    '{"symbol": "WTICO_USD", "timeframe": "D", "sl_pips": 400, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 2. Oil Bollinger D1 Wide (93.3% WR, Score 98.67)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a3000000-0000-0000-0002-000000000001',
    'Oil Bollinger D1 Wide',
    'bollinger_mean_reversion',
    '{"symbol": "WTICO_USD", "timeframe": "D", "sl_pips": 500, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 3. Oil RSI MR D1 (90.9% WR, Score 98.18)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a3000000-0000-0000-0003-000000000001',
    'Oil RSI MR D1',
    'rsi_mean_reversion',
    '{"symbol": "WTICO_USD", "timeframe": "D", "sl_pips": 1000, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 4. Oil MACD Divergence D1 (83.3% WR, Score 96.67)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a3000000-0000-0000-0004-000000000001',
    'Oil MACD Divergence D1',
    'macd_divergence',
    '{"symbol": "WTICO_USD", "timeframe": "D", "sl_pips": 1500, "tp_pips": 1500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 5. Oil Bollinger D1 Tight (80.8% WR, Score 96.15)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a3000000-0000-0000-0005-000000000001',
    'Oil Bollinger D1 Tight',
    'bollinger_mean_reversion',
    '{"symbol": "WTICO_USD", "timeframe": "D", "sl_pips": 250, "tp_pips": 500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- =====================================================
-- BOTS
-- =====================================================

-- 1. Oil Bollinger D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b3000000-0000-0000-0001-000000000001',
    'Oil Bollinger D1',
    'a3000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 2. Oil Bollinger D1 Wide Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b3000000-0000-0000-0002-000000000001',
    'Oil Bollinger D1 Wide',
    'a3000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 3. Oil RSI MR D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b3000000-0000-0000-0003-000000000001',
    'Oil RSI MR D1',
    'a3000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 4. Oil MACD Divergence D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b3000000-0000-0000-0004-000000000001',
    'Oil MACD Divergence D1',
    'a3000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 5. Oil Bollinger D1 Tight Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b3000000-0000-0000-0005-000000000001',
    'Oil Bollinger D1 Tight',
    'a3000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);
