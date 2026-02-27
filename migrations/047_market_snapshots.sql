-- Market price snapshots for movers detection
CREATE TABLE market_snapshots (
    id          BIGSERIAL PRIMARY KEY,
    market_id   TEXT NOT NULL,
    title       TEXT NOT NULL,
    outcome_name TEXT NOT NULL,
    price       DOUBLE PRECISION NOT NULL,
    volume_usd  DOUBLE PRECISION NOT NULL DEFAULT 0,
    image_url   TEXT,
    url         TEXT,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX market_snapshots_market_time ON market_snapshots(market_id, captured_at DESC);
CREATE INDEX market_snapshots_time ON market_snapshots(captured_at DESC);
