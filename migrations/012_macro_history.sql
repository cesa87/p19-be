-- Macro History Table for storing Polymarket probability snapshots
-- Used to calculate 7-day deltas for the Macro-Aligned Momentum strategy

CREATE TABLE IF NOT EXISTS macro_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Core probabilities from Polymarket (0.0 to 1.0)
    rate_cut_prob DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    rate_hike_prob DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    inflation_prob DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    recession_prob DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    btc_sentiment DOUBLE PRECISION NOT NULL DEFAULT 0.0,  -- -1 to +1
    
    -- Computed macro score and regime
    macro_score DOUBLE PRECISION NOT NULL DEFAULT 0.0,  -- -100 to +100
    macro_regime VARCHAR(20) NOT NULL DEFAULT 'NEUTRAL',  -- RISK_ON, RISK_OFF, NEUTRAL
    
    -- Position sizing multiplier based on conviction
    position_multiplier DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    
    -- Breakout mode tracking
    breakout_mode BOOLEAN NOT NULL DEFAULT FALSE,
    regime_flip_date TIMESTAMPTZ,  -- When the regime last flipped
    
    -- Metadata
    markets_analyzed INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for efficient time-based queries (getting 7-day lookback)
CREATE INDEX IF NOT EXISTS idx_macro_history_timestamp ON macro_history(timestamp DESC);

-- Index for getting latest snapshot
CREATE INDEX IF NOT EXISTS idx_macro_history_created ON macro_history(created_at DESC);

-- Function to automatically calculate macro_score before insert
-- macro_score = ΔRATE_CUT + ΔINFLATION + ΔCRISIS - (ΔRECESSION × 0.5)
-- Since we store raw probabilities, the score is computed at query time using deltas

-- View to get the latest macro state
CREATE OR REPLACE VIEW current_macro_state AS
SELECT * FROM macro_history
ORDER BY timestamp DESC
LIMIT 1;

-- View to calculate 7-day deltas and derive macro score
CREATE OR REPLACE VIEW macro_regime_analysis AS
WITH latest AS (
    SELECT * FROM macro_history
    ORDER BY timestamp DESC
    LIMIT 1
),
week_ago AS (
    SELECT * FROM macro_history
    WHERE timestamp <= NOW() - INTERVAL '7 days'
    ORDER BY timestamp DESC
    LIMIT 1
),
deltas AS (
    SELECT
        l.id,
        l.timestamp,
        l.rate_cut_prob,
        l.inflation_prob,
        l.recession_prob,
        l.btc_sentiment,
        -- Calculate deltas (multiply by 100 to get percentage points)
        (l.rate_cut_prob - COALESCE(w.rate_cut_prob, l.rate_cut_prob)) * 100 AS delta_rate_cut,
        (l.inflation_prob - COALESCE(w.inflation_prob, l.inflation_prob)) * 100 AS delta_inflation,
        (l.recession_prob - COALESCE(w.recession_prob, l.recession_prob)) * 100 AS delta_recession,
        (l.btc_sentiment - COALESCE(w.btc_sentiment, l.btc_sentiment)) * 100 AS delta_btc,
        l.markets_analyzed
    FROM latest l
    LEFT JOIN week_ago w ON TRUE
)
SELECT
    id,
    timestamp,
    rate_cut_prob,
    inflation_prob,
    recession_prob,
    btc_sentiment,
    delta_rate_cut,
    delta_inflation,
    delta_recession,
    delta_btc,
    -- Compute macro score: ΔRATE_CUT + ΔINFLATION - (ΔRECESSION × 0.5)
    -- Positive score = bullish (liquidity coming)
    -- Negative score = bearish (tightening)
    (delta_rate_cut + delta_inflation - (delta_recession * 0.5)) AS computed_macro_score,
    -- Determine regime
    CASE
        WHEN (delta_rate_cut + delta_inflation - (delta_recession * 0.5)) > 15 THEN 'RISK_ON'
        WHEN (delta_rate_cut + delta_inflation - (delta_recession * 0.5)) < -15 THEN 'RISK_OFF'
        ELSE 'NEUTRAL'
    END AS computed_regime,
    -- Position multiplier based on score magnitude
    CASE
        WHEN ABS(delta_rate_cut + delta_inflation - (delta_recession * 0.5)) > 40 THEN 2.0
        WHEN ABS(delta_rate_cut + delta_inflation - (delta_recession * 0.5)) > 25 THEN 1.5
        ELSE 1.0
    END AS computed_multiplier,
    markets_analyzed
FROM deltas;

-- Insert initial record so the system has a baseline
INSERT INTO macro_history (
    rate_cut_prob,
    rate_hike_prob,
    inflation_prob,
    recession_prob,
    btc_sentiment,
    macro_score,
    macro_regime,
    position_multiplier,
    breakout_mode,
    markets_analyzed
) VALUES (
    0.0, 0.0, 0.5, 0.5, 0.0,
    0.0, 'NEUTRAL', 1.0, FALSE, 0
);
