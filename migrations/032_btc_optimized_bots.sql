-- Migration: Create optimized Bitcoin bots based on backtest results
-- Date: 2026-02-18
-- Results from: aureum-optimizer-bitcoin-2026-02-15.json

-- System user ID for bot ownership
-- a0000000-0000-0000-0000-000000000001

-- =====================================================
-- STRATEGIES
-- =====================================================

-- 1. BTC Triple Screen H4 (83.3% WR, Score 91.75)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'c1000000-0000-0000-0001-000000000001',
    'BTC Triple Screen H4',
    'triple_screen',
    '{"symbol": "BTC_USD", "timeframe": "H4", "sl_pips": 1000, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 2. BTC Bollinger D1 (78.6% WR, Score 91.11)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'c1000000-0000-0000-0002-000000000001',
    'BTC Bollinger D1',
    'bollinger_mean_reversion',
    '{"symbol": "BTC_USD", "timeframe": "D", "sl_pips": 2500, "tp_pips": 2500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 3. BTC Bollinger D1 Wide (70.8% WR, Score 89.67)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'c1000000-0000-0000-0003-000000000001',
    'BTC Bollinger D1 Wide',
    'bollinger_mean_reversion',
    '{"symbol": "BTC_USD", "timeframe": "D", "sl_pips": 2500, "tp_pips": 4000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 4. BTC Bollinger H1 (70.3% WR, Score 89.20)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'c1000000-0000-0000-0004-000000000001',
    'BTC Bollinger H1',
    'bollinger_mean_reversion',
    '{"symbol": "BTC_USD", "timeframe": "H1", "sl_pips": 500, "tp_pips": 800}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 5. BTC Bollinger H4 (63.3% WR, Score 85.07)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'c1000000-0000-0000-0005-000000000001',
    'BTC Bollinger H4',
    'bollinger_mean_reversion',
    '{"symbol": "BTC_USD", "timeframe": "H4", "sl_pips": 1000, "tp_pips": 1600}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- =====================================================
-- BOTS
-- =====================================================

-- 1. BTC Triple Screen H4 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'd1000000-0000-0000-0001-000000000001',
    'BTC Triple Screen H4',
    'c1000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 2. BTC Bollinger D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'd1000000-0000-0000-0002-000000000001',
    'BTC Bollinger D1',
    'c1000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 3. BTC Bollinger D1 Wide Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'd1000000-0000-0000-0003-000000000001',
    'BTC Bollinger D1 Wide',
    'c1000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 4. BTC Bollinger H1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'd1000000-0000-0000-0004-000000000001',
    'BTC Bollinger H1',
    'c1000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 5. BTC Bollinger H4 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'd1000000-0000-0000-0005-000000000001',
    'BTC Bollinger H4',
    'c1000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);
