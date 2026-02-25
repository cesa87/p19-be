-- Tracked military/government aircraft from ADS-B sources

CREATE TABLE IF NOT EXISTS tracked_flights (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    icao24          VARCHAR(10)  NOT NULL,
    callsign        VARCHAR(20),
    aircraft_type   VARCHAR(20)  NOT NULL DEFAULT 'UNKNOWN',
    origin_country  VARCHAR(80),
    latitude        DOUBLE PRECISION,
    longitude       DOUBLE PRECISION,
    altitude_ft     DOUBLE PRECISION,
    speed_kts       DOUBLE PRECISION,
    heading         DOUBLE PRECISION,
    squawk          VARCHAR(10),
    on_ground       BOOLEAN      NOT NULL DEFAULT false,
    source          VARCHAR(20)  NOT NULL DEFAULT 'opensky',
    first_seen_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    last_seen_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT tracked_flights_icao24_unique UNIQUE (icao24)
);

CREATE INDEX IF NOT EXISTS idx_tracked_flights_type ON tracked_flights (aircraft_type);
CREATE INDEX IF NOT EXISTS idx_tracked_flights_last_seen ON tracked_flights (last_seen_at DESC);
