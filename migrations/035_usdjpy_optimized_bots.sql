-- Migration: Create optimized USD/JPY bots based on backtest results
-- Date: 2026-02-18
-- Results from: aureum-optimizer-USD:JPY-2026-02-15.json

-- =====================================================
-- STRATEGIES
-- =====================================================

-- 1. JPY Bollinger H4 (92.9% WR, Score 98.57)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a2000000-0000-0000-0001-000000000001',
    'JPY Bollinger H4',
    'bollinger_mean_reversion',
    '{"symbol": "USD_JPY", "timeframe": "H4", "sl_pips": 100, "tp_pips": 200}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 2. JPY SMA Crossover H4 (84.2% WR, Score 96.84)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a2000000-0000-0000-0002-000000000001',
    'JPY SMA Crossover H4',
    'sma_crossover',
    '{"symbol": "USD_JPY", "timeframe": "H4", "sl_pips": 160, "tp_pips": 200}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 3. JPY Bollinger D1 (81.8% WR, Score 96.36)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a2000000-0000-0000-0003-000000000001',
    'JPY Bollinger D1',
    'bollinger_mean_reversion',
    '{"symbol": "USD_JPY", "timeframe": "D", "sl_pips": 750, "tp_pips": 750}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 4. JPY MACD Divergence D1 (80% WR, Score 96.00)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a2000000-0000-0000-0004-000000000001',
    'JPY MACD Divergence D1',
    'macd_divergence',
    '{"symbol": "USD_JPY", "timeframe": "D", "sl_pips": 1000, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 5. JPY Bollinger M15 (80% WR, Score 96.00)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a2000000-0000-0000-0005-000000000001',
    'JPY Bollinger M15',
    'bollinger_mean_reversion',
    '{"symbol": "USD_JPY", "timeframe": "M15", "sl_pips": 32, "tp_pips": 40}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- =====================================================
-- BOTS
-- =====================================================

-- 1. JPY Bollinger H4 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b2000000-0000-0000-0001-000000000001',
    'JPY Bollinger H4',
    'a2000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 2. JPY SMA Crossover H4 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b2000000-0000-0000-0002-000000000001',
    'JPY SMA Crossover H4',
    'a2000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 3. JPY Bollinger D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b2000000-0000-0000-0003-000000000001',
    'JPY Bollinger D1',
    'a2000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 4. JPY MACD Divergence D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b2000000-0000-0000-0004-000000000001',
    'JPY MACD Divergence D1',
    'a2000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 5. JPY Bollinger M15 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b2000000-0000-0000-0005-000000000001',
    'JPY Bollinger M15',
    'a2000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);
