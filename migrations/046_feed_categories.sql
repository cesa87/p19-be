-- Migration 046: Feed source categories, reliability scores, and tier ranking
-- Supports tab filtering: All / News / Telegram / OSINT / X

ALTER TABLE feed_sources
    ADD COLUMN IF NOT EXISTS category        VARCHAR(20)  NOT NULL DEFAULT 'telegram',
    ADD COLUMN IF NOT EXISTS reliability     FLOAT        NOT NULL DEFAULT 0.70,
    ADD COLUMN IF NOT EXISTS tier            INT          NOT NULL DEFAULT 2;

-- ─── Update existing 24 sources ───────────────────────────────────────────────

UPDATE feed_sources SET category='news',     reliability=0.90, tier=1 WHERE handle='Walter_Bloomberg';
UPDATE feed_sources SET category='telegram', reliability=0.70, tier=4 WHERE handle='CryptoWhaleAlerts';
UPDATE feed_sources SET category='telegram', reliability=0.82, tier=4 WHERE handle='MacroAlf';
UPDATE feed_sources SET category='telegram', reliability=0.78, tier=4 WHERE handle='polymarketupdates';
UPDATE feed_sources SET category='telegram', reliability=0.72, tier=4 WHERE handle='marketsmayhem';
UPDATE feed_sources SET category='telegram', reliability=0.78, tier=4 WHERE handle='realvisionfinance';
UPDATE feed_sources SET category='osint',    reliability=0.55, tier=3 WHERE handle='intelslava';      -- pro-RU bias flagged
UPDATE feed_sources SET category='osint',    reliability=0.80, tier=3 WHERE handle='nexta_live';
UPDATE feed_sources SET category='osint',    reliability=0.75, tier=3 WHERE handle='ukrainewarmap';
UPDATE feed_sources SET category='osint',    reliability=0.72, tier=3 WHERE handle='militarysummary';
UPDATE feed_sources SET category='osint',    reliability=0.80, tier=2 WHERE handle='WarMonitor';
UPDATE feed_sources SET category='osint',    reliability=0.70, tier=3 WHERE handle='arabgulfmonitor';
UPDATE feed_sources SET category='osint',    reliability=0.62, tier=3 WHERE handle='PalestineChronicle';
UPDATE feed_sources SET category='telegram', reliability=0.75, tier=4 WHERE handle='thesignalcentre';
UPDATE feed_sources SET category='telegram', reliability=0.75, tier=4 WHERE handle='euromacrowatch';
UPDATE feed_sources SET category='osint',    reliability=0.75, tier=2 WHERE handle='geopolitics_live';
UPDATE feed_sources SET category='osint',    reliability=0.60, tier=3 WHERE handle='rybar_en';        -- pro-RU bias flagged
UPDATE feed_sources SET category='news',     reliability=0.72, tier=4 WHERE handle='asia_markets_news';
UPDATE feed_sources SET category='news',     reliability=0.82, tier=3 WHERE handle='EconomicTimesApp';
UPDATE feed_sources SET category='news',     reliability=0.85, tier=3 WHERE handle='scmpnews';
UPDATE feed_sources SET category='telegram', reliability=0.80, tier=4 WHERE handle='otaviocosta_';
UPDATE feed_sources SET category='news',     reliability=0.65, tier=3 WHERE handle='latam_daily';
UPDATE feed_sources SET category='osint',    reliability=0.70, tier=3 WHERE handle='africaintelligence';
UPDATE feed_sources SET category='telegram', reliability=0.72, tier=4 WHERE handle='commoditywatch';

-- ─── Tier 1: Fast Breaking News & Financials ──────────────────────────────────

INSERT INTO feed_sources (source_type, name, handle, default_severity, category, reliability, tier, latitude, longitude, country_code)
VALUES
  ('telegram','Bloomberg',            'bloomberg',          'HIGH',   'news',     0.95, 1,  40.7,  -74.0, 'US'),
  ('telegram','Reuters',              'reuters',            'HIGH',   'news',     0.97, 1,  51.5,   -0.1, 'GB'),
  ('telegram','Spectator Index',      'spectatorindex',     'MEDIUM', 'news',     0.85, 1,   0.0,    0.0, NULL),
  ('telegram','Semafor',              'Semafor',            'MEDIUM', 'news',     0.85, 1,  40.7,  -74.0, 'US'),
  ('telegram','ZeroHedge',            'zerohedge',          'HIGH',   'news',     0.55, 1,  40.7,  -74.0, 'US'),
  ('telegram','Wall Street Journal',  'wsj',                'HIGH',   'news',     0.93, 1,  40.7,  -74.0, 'US')
ON CONFLICT (source_type, handle) DO UPDATE SET
  name=EXCLUDED.name, category=EXCLUDED.category, reliability=EXCLUDED.reliability,
  tier=EXCLUDED.tier, latitude=EXCLUDED.latitude, longitude=EXCLUDED.longitude;

-- ─── Tier 2: Geopolitics & Conflict ──────────────────────────────────────────

INSERT INTO feed_sources (source_type, name, handle, default_severity, category, reliability, tier, latitude, longitude, country_code)
VALUES
  ('telegram','Intel Republic',       'IntelRepublic',      'HIGH',   'osint',    0.75, 2,  38.9,  -77.0, 'US'),
  ('telegram','Geopolitical Futures', 'geopoliticalfutures','MEDIUM', 'osint',    0.82, 2,  30.3,  -97.7, 'US'),
  ('telegram','Caspian Report',       'CaspianReport',      'MEDIUM', 'osint',    0.78, 2,  41.8,   49.7, NULL),
  ('telegram','Inside Paper',         'insidepaper',        'HIGH',   'osint',    0.72, 2,   0.0,    0.0, NULL),
  ('telegram','Disclose TV',          'disclosetv',         'HIGH',   'osint',    0.65, 2,  47.6,    7.6, 'CH'),
  ('telegram','Sentinel Defender',    'sentdefender',       'HIGH',   'osint',    0.72, 2,  38.9,  -77.0, 'US')
ON CONFLICT (source_type, handle) DO UPDATE SET
  name=EXCLUDED.name, category=EXCLUDED.category, reliability=EXCLUDED.reliability,
  tier=EXCLUDED.tier, latitude=EXCLUDED.latitude, longitude=EXCLUDED.longitude;

-- ─── Tier 3: Region-Specific ─────────────────────────────────────────────────

INSERT INTO feed_sources (source_type, name, handle, default_severity, category, reliability, tier, latitude, longitude, country_code)
VALUES
  ('telegram','Middle East Eye',      'MiddleEastEye',      'HIGH',   'osint',    0.72, 3,  25.0,   45.0, NULL),
  ('telegram','Kyiv Post',            'KyivPost',           'HIGH',   'news',     0.80, 3,  50.4,   30.5, 'UA'),
  ('telegram','Astra Press',          'astrapress',         'HIGH',   'osint',    0.75, 3,  55.7,   37.6, 'RU')
ON CONFLICT (source_type, handle) DO UPDATE SET
  name=EXCLUDED.name, category=EXCLUDED.category, reliability=EXCLUDED.reliability,
  tier=EXCLUDED.tier, latitude=EXCLUDED.latitude, longitude=EXCLUDED.longitude;

-- ─── Tier 4: Trading & Economic Data ─────────────────────────────────────────

INSERT INTO feed_sources (source_type, name, handle, default_severity, category, reliability, tier, latitude, longitude, country_code)
VALUES
  ('telegram','FXStreet',             'fxstreet',           'HIGH',   'news',     0.88, 4,  41.4,    2.2, 'ES'),
  ('telegram','CoinDesk',             'CoinDesk',           'MEDIUM', 'news',     0.85, 4,  40.7,  -74.0, 'US'),
  ('telegram','Whale Alert',          'whale_alert',        'HIGH',   'telegram', 0.88, 4,  52.4,    4.9, 'NL'),
  ('telegram','OilPrice.com',         'OilPrice_com',       'MEDIUM', 'news',     0.80, 4,  40.7,  -74.0, 'US')
ON CONFLICT (source_type, handle) DO UPDATE SET
  name=EXCLUDED.name, category=EXCLUDED.category, reliability=EXCLUDED.reliability,
  tier=EXCLUDED.tier, latitude=EXCLUDED.latitude, longitude=EXCLUDED.longitude;
