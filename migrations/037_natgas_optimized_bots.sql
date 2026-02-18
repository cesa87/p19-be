-- Migration: Create optimized Natural Gas bots based on backtest results
-- Date: 2026-02-18
-- Results from: aureum-optimizer-NG-2026-02-16.json

-- =====================================================
-- STRATEGIES
-- =====================================================

-- 1. NatGas Bollinger D1 (88.6% WR, Score 97.71)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a4000000-0000-0000-0001-000000000001',
    'NatGas Bollinger D1',
    'bollinger_mean_reversion',
    '{"symbol": "NATGAS_USD", "timeframe": "D", "sl_pips": 500, "tp_pips": 500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 2. NatGas Bollinger M15 (94.4% WR, Score 97.49)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a4000000-0000-0000-0002-000000000001',
    'NatGas Bollinger M15',
    'bollinger_mean_reversion',
    '{"symbol": "NATGAS_USD", "timeframe": "M15", "sl_pips": 120, "tp_pips": 120}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 3. NatGas Bollinger H1 (92.2% WR, Score 97.40)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a4000000-0000-0000-0003-000000000001',
    'NatGas Bollinger H1',
    'bollinger_mean_reversion',
    '{"symbol": "NATGAS_USD", "timeframe": "H1", "sl_pips": 100, "tp_pips": 100}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 4. NatGas Bollinger H4 (89.7% WR, Score 97.34)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a4000000-0000-0000-0004-000000000001',
    'NatGas Bollinger H4',
    'bollinger_mean_reversion',
    '{"symbol": "NATGAS_USD", "timeframe": "H4", "sl_pips": 200, "tp_pips": 200}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 5. NatGas Bollinger D1 Wide (85.3% WR, Score 97.06)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'a4000000-0000-0000-0005-000000000001',
    'NatGas Bollinger D1 Wide',
    'bollinger_mean_reversion',
    '{"symbol": "NATGAS_USD", "timeframe": "D", "sl_pips": 400, "tp_pips": 500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- =====================================================
-- BOTS
-- =====================================================

-- 1. NatGas Bollinger D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b4000000-0000-0000-0001-000000000001',
    'NatGas Bollinger D1',
    'a4000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 2. NatGas Bollinger M15 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b4000000-0000-0000-0002-000000000001',
    'NatGas Bollinger M15',
    'a4000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 3. NatGas Bollinger H1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b4000000-0000-0000-0003-000000000001',
    'NatGas Bollinger H1',
    'a4000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 4. NatGas Bollinger H4 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b4000000-0000-0000-0004-000000000001',
    'NatGas Bollinger H4',
    'a4000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 5. NatGas Bollinger D1 Wide Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'b4000000-0000-0000-0005-000000000001',
    'NatGas Bollinger D1 Wide',
    'a4000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);
