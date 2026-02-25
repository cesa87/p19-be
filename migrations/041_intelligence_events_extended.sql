-- Extend intelligence_events with spike detection columns
ALTER TABLE intelligence_events
    ADD COLUMN IF NOT EXISTS value DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS baseline_value DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS std_dev DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS z_score DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS affected_instruments TEXT;
