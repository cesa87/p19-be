-- MRATE Engine Improvements Migration
-- Adds columns needed for the 20-point MRATE improvement plan

-- #14: Orchestrator auto-enable/disable tracking
-- When the orchestrator auto-disables a bot due to unfavorable conditions,
-- this flag is set so it can auto-re-enable when conditions improve.
-- Bots manually disabled by the user will NOT have this flag set.
ALTER TABLE bots ADD COLUMN IF NOT EXISTS auto_disabled BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN bots.auto_disabled IS 
'True if bot was automatically disabled by the orchestrator. Used to auto-re-enable when conditions improve.';

-- #16: Persist instrument scores in MRATE snapshots
-- Stores the full instrument scoring breakdown for historical analysis
ALTER TABLE mrate_snapshots ADD COLUMN IF NOT EXISTS instrument_scores JSONB;

COMMENT ON COLUMN mrate_snapshots.instrument_scores IS 
'JSONB containing per-instrument scores (gold, bitcoin, eur_usd, usd_jpy, wti_oil, natural_gas) with score, recommendation, and factors';
