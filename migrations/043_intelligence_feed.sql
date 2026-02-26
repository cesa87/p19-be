-- ─────────────────────────────────────────────────────────────────────────────
-- Migration 043: Intelligence Feed Sources + Posts
-- Stores configurable channel sources and ingested posts with severity scoring.
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS feed_sources (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    source_type     VARCHAR(20) NOT NULL DEFAULT 'telegram',  -- telegram | twitter | rss
    name            VARCHAR(255) NOT NULL,
    handle          VARCHAR(255) NOT NULL,                   -- @channelname / username / url
    enabled         BOOLEAN     NOT NULL DEFAULT TRUE,
    default_severity VARCHAR(20) NOT NULL DEFAULT 'MEDIUM',  -- floor severity for posts
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_type, handle)
);

CREATE TABLE IF NOT EXISTS feed_posts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id       UUID        NOT NULL REFERENCES feed_sources(id) ON DELETE CASCADE,
    external_id     VARCHAR(255) NOT NULL,                   -- message_id from source
    author          VARCHAR(255) NOT NULL,
    content         TEXT        NOT NULL,
    image_url       TEXT,
    severity        VARCHAR(20) NOT NULL DEFAULT 'LOW',      -- LOW | MEDIUM | HIGH | CRITICAL
    severity_reason TEXT,
    related_markets TEXT[]      NOT NULL DEFAULT '{}',       -- Polymarket market slugs
    source_url      TEXT,
    posted_at       TIMESTAMPTZ NOT NULL,
    ingested_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_id, external_id)
);

CREATE INDEX IF NOT EXISTS idx_feed_posts_posted_at  ON feed_posts (posted_at DESC);
CREATE INDEX IF NOT EXISTS idx_feed_posts_severity   ON feed_posts (severity, posted_at DESC);
CREATE INDEX IF NOT EXISTS idx_feed_posts_source     ON feed_posts (source_id, posted_at DESC);

-- Default Telegram sources (market-intelligence focused channels)
INSERT INTO feed_sources (source_type, name, handle, enabled, default_severity) VALUES
    ('telegram', 'Walter Bloomberg',        'Walter_Bloomberg',      true,  'HIGH'),
    ('telegram', 'Polymarket Updates',      'polymarketupdates',     true,  'MEDIUM'),
    ('telegram', 'Markets & Mayhem',        'marketsmayhem',         true,  'HIGH'),
    ('telegram', 'Crypto Whale Alerts',     'CryptoWhaleAlerts',     true,  'MEDIUM'),
    ('telegram', 'Real Vision Finance',     'realvisionfinance',     true,  'MEDIUM'),
    ('telegram', 'MacroAlf',                'MacroAlf',              true,  'HIGH')
ON CONFLICT (source_type, handle) DO NOTHING;
