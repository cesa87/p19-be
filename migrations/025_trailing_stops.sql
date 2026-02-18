-- Trailing Stop State Tracking
-- Stores real-time state for ML-powered trailing stops

CREATE TABLE IF NOT EXISTS trailing_stop_state (
    position_id UUID PRIMARY KEY,
    instrument TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('long', 'short')),
    
    -- Price levels
    entry_price NUMERIC NOT NULL,
    original_stop NUMERIC NOT NULL,
    original_tp NUMERIC NOT NULL,
    current_stop NUMERIC NOT NULL,
    current_tp NUMERIC NOT NULL,
    
    -- Trail configuration
    atr NUMERIC NOT NULL,
    trail_distance_atr NUMERIC NOT NULL,
    
    -- State flags
    trailing_active BOOLEAN NOT NULL DEFAULT false,
    tp_extended BOOLEAN NOT NULL DEFAULT false,
    
    -- High/low watermarks
    highest_price NUMERIC NOT NULL,
    lowest_price NUMERIC NOT NULL,
    
    -- ML adjustments applied
    regime_adjustment TEXT NOT NULL DEFAULT 'none',
    correlation_adjustment NUMERIC NOT NULL DEFAULT 1.0,
    sentiment_adjustment NUMERIC NOT NULL DEFAULT 1.0,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for quickly finding active trails
CREATE INDEX idx_trailing_stop_active ON trailing_stop_state(trailing_active) WHERE trailing_active = true;

-- Index for instrument-based queries
CREATE INDEX idx_trailing_stop_instrument ON trailing_stop_state(instrument);

-- Index for recent updates
CREATE INDEX idx_trailing_stop_updated ON trailing_stop_state(last_updated DESC);

-- Comments
COMMENT ON TABLE trailing_stop_state IS 'Real-time state tracking for ML-powered trailing stops';
COMMENT ON COLUMN trailing_stop_state.trail_distance_atr IS 'Current trailing distance in ATR multiples (adjusted by ML)';
COMMENT ON COLUMN trailing_stop_state.regime_adjustment IS 'Human-readable reason for regime-based adjustment';
COMMENT ON COLUMN trailing_stop_state.correlation_adjustment IS 'Multiplier applied due to portfolio correlation (0.7-1.0)';
COMMENT ON COLUMN trailing_stop_state.sentiment_adjustment IS 'Multiplier applied for TP extension based on sentiment (1.0-1.3)';
