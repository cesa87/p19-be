-- MRATE (Macro-Regime Adaptive Trading Engine) Tables
-- Migration 014

-- MRATE Snapshots - historical regime data (optional persistence)
CREATE TABLE IF NOT EXISTS mrate_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL,
    regime TEXT NOT NULL,
    liquidity_score REAL NOT NULL,
    risk_score REAL NOT NULL,
    uncertainty_score REAL NOT NULL,
    risk_multiplier REAL NOT NULL,
    strategy_weights JSONB NOT NULL DEFAULT '{}',
    raw_inputs JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for time-based queries
CREATE INDEX IF NOT EXISTS idx_mrate_snapshots_timestamp 
ON mrate_snapshots(timestamp DESC);

-- Index for regime filtering
CREATE INDEX IF NOT EXISTS idx_mrate_snapshots_regime 
ON mrate_snapshots(regime);

-- Add MRATE category to strategies table
-- Values: trend, breakout, mean_reversion, liquidity_sweep, NULL (disabled)
ALTER TABLE strategies 
ADD COLUMN IF NOT EXISTS mrate_category TEXT;

-- Add comment explaining the column
COMMENT ON COLUMN strategies.mrate_category IS 
'MRATE strategy category for weighting: trend, breakout, mean_reversion, liquidity_sweep, or NULL to disable';

-- Clean up old snapshots (keep last 7 days by default)
-- This can be run periodically as a maintenance task
-- DELETE FROM mrate_snapshots WHERE timestamp < NOW() - INTERVAL '7 days';
