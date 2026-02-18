-- MRATE Activity Log
-- Records what the MRATE engine is doing/seeing for transparency

CREATE TABLE IF NOT EXISTS mrate_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMP NOT NULL DEFAULT NOW(),
    activity_type VARCHAR(50) NOT NULL,  -- 'feed_fetch', 'regime_change', 'score_update', 'data_warning', 'decision'
    source VARCHAR(50),                   -- 'polymarket', 'vix', 'fred', 'reddit', 'oanda', etc.
    message TEXT NOT NULL,
    details JSONB,                        -- Raw data values, scores, etc.
    severity VARCHAR(20) DEFAULT 'info'   -- 'info', 'warning', 'error', 'success'
);

CREATE INDEX idx_mrate_activities_timestamp ON mrate_activities(timestamp DESC);
CREATE INDEX idx_mrate_activities_type ON mrate_activities(activity_type, timestamp DESC);

-- Auto-cleanup old logs (keep 7 days)
CREATE OR REPLACE FUNCTION cleanup_old_mrate_activities() RETURNS void AS $$
BEGIN
    DELETE FROM mrate_activities WHERE timestamp < NOW() - INTERVAL '7 days';
END;
$$ LANGUAGE plpgsql;
