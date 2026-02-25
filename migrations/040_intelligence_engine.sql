-- Intelligence Engine Tables
-- Feed snapshots: normalised output from every data source
CREATE TABLE IF NOT EXISTS intelligence_feed_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    feed_id VARCHAR(64) NOT NULL,        -- e.g. "vix", "fear_greed", "polymarket_war"
    feed_source VARCHAR(64) NOT NULL,    -- e.g. "cboe", "alternative_me", "polymarket"
    value DOUBLE PRECISION NOT NULL,     -- normalised 0-100 or raw value
    label VARCHAR(255),                  -- human readable e.g. "VIX: 18.4 (Moderate Fear)"
    raw_json JSONB,                      -- full response for debugging
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_feed_snapshots_feed_id ON intelligence_feed_snapshots(feed_id, fetched_at DESC);
CREATE INDEX IF NOT EXISTS idx_feed_snapshots_fetched ON intelligence_feed_snapshots(fetched_at DESC);

-- Intelligence scores per instrument
CREATE TABLE IF NOT EXISTS intelligence_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instrument VARCHAR(32) NOT NULL,
    tension_score DOUBLE PRECISION NOT NULL DEFAULT 0,      -- 0-100, geopolitical/macro risk
    opportunity_score DOUBLE PRECISION NOT NULL DEFAULT 0,  -- 0-100, trade signal strength
    direction_bias VARCHAR(8),                               -- 'LONG', 'SHORT', or NULL
    confidence DOUBLE PRECISION NOT NULL DEFAULT 0,         -- 0-1
    top_signals JSONB,                                       -- [{feed, description, impact}]
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_intelligence_scores_instrument ON intelligence_scores(instrument, created_at DESC);

-- Intelligence events: spike detections and anomalies
CREATE TABLE IF NOT EXISTS intelligence_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    feed_id VARCHAR(64),
    instrument VARCHAR(32),
    event_type VARCHAR(64) NOT NULL,    -- e.g. "SPIKE", "THRESHOLD_BREACH", "ARB_DETECTED", "WHALE_MOVE"
    description TEXT NOT NULL,
    severity VARCHAR(16) NOT NULL,      -- 'LOW', 'MEDIUM', 'HIGH', 'CRITICAL'
    value_before DOUBLE PRECISION,
    value_after DOUBLE PRECISION,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_intelligence_events_detected ON intelligence_events(detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_intelligence_events_instrument ON intelligence_events(instrument, detected_at DESC);

-- Whale alerts: large on-chain transactions
CREATE TABLE IF NOT EXISTS whale_alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    blockchain VARCHAR(32) NOT NULL,
    symbol VARCHAR(16) NOT NULL,
    amount_usd DOUBLE PRECISION NOT NULL,
    amount_native DOUBLE PRECISION,
    from_label VARCHAR(255),            -- e.g. "Unknown Wallet", "Binance"
    to_label VARCHAR(255),              -- e.g. "Coinbase", "Unknown Wallet"
    tx_hash VARCHAR(128),
    market_impact VARCHAR(16),          -- 'BULLISH', 'BEARISH', 'NEUTRAL'
    impact_reason TEXT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_whale_alerts_detected ON whale_alerts(detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_whale_alerts_symbol ON whale_alerts(symbol, detected_at DESC);

-- Arb opportunities: prediction market spread detection
CREATE TABLE IF NOT EXISTS arb_opportunities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source VARCHAR(32) NOT NULL,        -- 'polymarket', 'kalshi'
    market_id VARCHAR(255) NOT NULL,
    market_name TEXT NOT NULL,
    yes_price DOUBLE PRECISION NOT NULL,
    no_price DOUBLE PRECISION NOT NULL,
    spread_pct DOUBLE PRECISION NOT NULL,   -- (1 - yes - no) * 100
    estimated_profit_per_100 DOUBLE PRECISION, -- $ profit per $100 deployed
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ,              -- when spread closed back to >= 0.995
    status VARCHAR(16) NOT NULL DEFAULT 'OPEN'  -- 'OPEN', 'CLOSED', 'EXPIRED'
);
CREATE INDEX IF NOT EXISTS idx_arb_opportunities_status ON arb_opportunities(status, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_arb_opportunities_source ON arb_opportunities(source, detected_at DESC);
