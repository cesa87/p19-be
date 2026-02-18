-- Migration: Add locked_directions table for institutional D1 direction approach
-- Date: 2026-02-18
-- 
-- Direction is locked once per day at NY close (10pm GMT / 5pm EST)
-- Based on D1 close price vs 200 EMA
-- Bots read locked direction, not live price

CREATE TABLE IF NOT EXISTS locked_directions (
    instrument VARCHAR(20) PRIMARY KEY,
    direction VARCHAR(10) NOT NULL,  -- 'LONG', 'SHORT', 'NEUTRAL'
    d1_close_price DECIMAL(20, 6) NOT NULL,
    ema_200 DECIMAL(20, 6) NOT NULL,
    locked_at TIMESTAMP WITH TIME ZONE NOT NULL,
    next_update_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for quick lookups
CREATE INDEX IF NOT EXISTS idx_locked_directions_next_update 
ON locked_directions(next_update_at);

-- Insert initial directions based on current state (will be updated at next NY close)
-- These are placeholder values - the scheduler will update them properly
INSERT INTO locked_directions (instrument, direction, d1_close_price, ema_200, locked_at, next_update_at)
VALUES 
    ('XAU_USD', 'LONG', 0, 0, NOW(), NOW()),
    ('BTC_USD', 'SHORT', 0, 0, NOW(), NOW()),
    ('EUR_USD', 'LONG', 0, 0, NOW(), NOW()),
    ('USD_JPY', 'NEUTRAL', 0, 0, NOW(), NOW()),
    ('WTICO_USD', 'NEUTRAL', 0, 0, NOW(), NOW()),
    ('NATGAS_USD', 'NEUTRAL', 0, 0, NOW(), NOW())
ON CONFLICT (instrument) DO NOTHING;
