-- Migration 049: English OSINT Telegram channels
-- These are high-signal, English-language sources used by intelligence terminals
-- to detect geopolitical/military events before they hit mainstream news

INSERT INTO feed_sources
  (source_type, name, handle, tier, reliability, default_severity, category,
   enabled, latitude, longitude, country_code)
VALUES
  -- Military conflict / kinetic event trackers
  ('telegram', 'Clash Report',      'clashreport',     1, 0.82, 'HIGH',   'osint',       true,  38.90, -77.03, 'us'),
  ('telegram', 'Intel Republic',    'IntelRepublic',   1, 0.80, 'HIGH',   'osint',       true,  38.90, -77.03, 'us'),
  ('telegram', 'LiveUAMap',         'liveuamap',       1, 0.85, 'HIGH',   'osint',       true,  50.45,  30.52, 'ua'),
  ('telegram', 'War Translated',    'wartranslated',   1, 0.78, 'HIGH',   'osint',       true,  50.45,  30.52, 'ua'),
  ('telegram', 'OSINT Technical',   'OSINTtechnical',  2, 0.75, 'MEDIUM', 'osint',       true,  38.90, -77.03, 'us'),

  -- Middle East / Israel-specific
  ('telegram', 'Middle East Eye',   'MiddleEastEye',   2, 0.72, 'MEDIUM', 'geopolitics', true,  31.77,  35.21, 'il'),
  ('telegram', 'Intel Slava Z',     'intelslava',      2, 0.65, 'HIGH',   'osint',       true,  55.75,  37.62, 'ru'),

  -- Breaking news flash style (all-caps headline format)
  ('telegram', 'Flash News',        'FlashNewsX',      2, 0.70, 'HIGH',   'news',        true,  40.75, -74.01, 'us'),
  ('telegram', 'Breaking News 24',  'BreakingNews24H', 2, 0.68, 'HIGH',   'news',        true,  40.75, -74.01, 'us'),

  -- Financial/markets
  ('telegram', 'Unusual Whales',    'unusual_whales',  1, 0.85, 'HIGH',   'markets',     true,  40.75, -74.01, 'us'),
  ('telegram', 'Whale Alert',       'whale_alert',     2, 0.78, 'MEDIUM', 'crypto',      true,  40.75, -74.01, 'us')

ON CONFLICT (source_type, handle) DO UPDATE
  SET reliability = EXCLUDED.reliability,
      tier        = EXCLUDED.tier,
      updated_at  = NOW();
