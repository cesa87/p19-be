-- Add Polymarket hex condition_id to market_snapshots so the frontend
-- can use it to look up token IDs on the CLOB API.
ALTER TABLE market_snapshots
  ADD COLUMN IF NOT EXISTS condition_id TEXT NOT NULL DEFAULT '';
