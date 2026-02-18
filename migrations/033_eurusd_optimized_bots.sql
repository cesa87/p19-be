-- Migration: Create optimized EUR/USD bots based on backtest results
-- Date: 2026-02-18
-- Results from: aureum-optimizer-EUR-2026-02-17.json

-- =====================================================
-- STRATEGIES
-- =====================================================

-- 1. EUR Bollinger M15 (75% WR, Score 90.33)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'e1000000-0000-0000-0001-000000000001',
    'EUR Bollinger M15',
    'bollinger_mean_reversion',
    '{"symbol": "EUR_USD", "timeframe": "M15", "sl_pips": 32, "tp_pips": 40}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 2. EUR MACD Divergence D1 (50% WR, Score 90.00)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'e1000000-0000-0000-0002-000000000001',
    'EUR MACD Divergence D1',
    'macd_divergence',
    '{"symbol": "EUR_USD", "timeframe": "D", "sl_pips": 250, "tp_pips": 1500}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 3. EUR EMA Ribbon M15 (70% WR, Score 88.56)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'e1000000-0000-0000-0003-000000000001',
    'EUR EMA Ribbon M15',
    'ema_ribbon',
    '{"symbol": "EUR_USD", "timeframe": "M15", "sl_pips": 80, "tp_pips": 80}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 4. EUR Bollinger D1 (46% WR, Score 87.89)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'e1000000-0000-0000-0004-000000000001',
    'EUR Bollinger D1',
    'bollinger_mean_reversion',
    '{"symbol": "EUR_USD", "timeframe": "D", "sl_pips": 250, "tp_pips": 1000}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- 5. EUR ADX Trend M15 (66.7% WR, Score 82.70)
INSERT INTO strategies (id, name, strategy_type, params, user_id, created_at)
VALUES (
    'e1000000-0000-0000-0005-000000000001',
    'EUR ADX Trend M15',
    'adx_trend',
    '{"symbol": "EUR_USD", "timeframe": "M15", "sl_pips": 80, "tp_pips": 80}',
    'a0000000-0000-0000-0000-000000000001',
    NOW()
);

-- =====================================================
-- BOTS
-- =====================================================

-- 1. EUR Bollinger M15 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'f1000000-0000-0000-0001-000000000001',
    'EUR Bollinger M15',
    'e1000000-0000-0000-0001-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 2. EUR MACD Divergence D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'f1000000-0000-0000-0002-000000000001',
    'EUR MACD Divergence D1',
    'e1000000-0000-0000-0002-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 3. EUR EMA Ribbon M15 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'f1000000-0000-0000-0003-000000000001',
    'EUR EMA Ribbon M15',
    'e1000000-0000-0000-0003-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 4. EUR Bollinger D1 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'f1000000-0000-0000-0004-000000000001',
    'EUR Bollinger D1',
    'e1000000-0000-0000-0004-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);

-- 5. EUR ADX Trend M15 Bot
INSERT INTO bots (id, name, strategy_id, user_id, is_active, auto_disabled, created_at)
VALUES (
    'f1000000-0000-0000-0005-000000000001',
    'EUR ADX Trend M15',
    'e1000000-0000-0000-0005-000000000001',
    'a0000000-0000-0000-0000-000000000001',
    true,
    false,
    NOW()
);
