-- Migration 048: RSS/Atom feed source support
-- Adds feed_url column + seeds ~30 high-quality RSS sources

-- 1. Schema change
ALTER TABLE feed_sources ADD COLUMN IF NOT EXISTS feed_url TEXT;

-- 2. Seed RSS sources
-- Tier 1: Major wire services
INSERT INTO feed_sources (source_type, name, handle, feed_url, tier, reliability, default_severity, category, enabled, latitude, longitude, country_code)
VALUES
  ('rss', 'Reuters – Top News',    'reuters_top',      'https://feeds.reuters.com/reuters/topNews',                     1, 0.97, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'Reuters – World',       'reuters_world',    'https://feeds.reuters.com/Reuters/worldNews',                   1, 0.96, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'Reuters – Business',    'reuters_biz',      'https://feeds.reuters.com/reuters/businessNews',                1, 0.95, 'MEDIUM', 'markets',    true,  40.75, -74.01, 'us'),
  ('rss', 'Associated Press',      'ap_topnews',       'https://feeds.apnews.com/rss/apf-topnews',                      1, 0.97, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'AP – Politics',         'ap_politics',      'https://feeds.apnews.com/rss/apf-politics',                     1, 0.95, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'AP – World News',       'ap_world',         'https://feeds.apnews.com/rss/apf-worldnews',                    1, 0.95, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'BBC – World',           'bbc_world',        'https://feeds.bbci.co.uk/news/world/rss.xml',                   1, 0.95, 'MEDIUM', 'news',       true,  51.50,  -0.12, 'gb'),
  ('rss', 'BBC – Technology',      'bbc_tech',         'https://feeds.bbci.co.uk/news/technology/rss.xml',              1, 0.92, 'LOW',    'tech',       true,  51.50,  -0.12, 'gb'),
  ('rss', 'NYT – World',           'nyt_world',        'https://rss.nytimes.com/services/xml/rss/nyt/World.xml',        1, 0.93, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),
  ('rss', 'NYT – Politics',        'nyt_politics',     'https://rss.nytimes.com/services/xml/rss/nyt/Politics.xml',     1, 0.92, 'MEDIUM', 'news',       true,  40.75, -74.01, 'us'),

-- Tier 1: US Politics (core for Polymarket)
  ('rss', 'Politico',              'politico_rss',     'https://www.politico.com/rss/politics08.xml',                   1, 0.88, 'MEDIUM', 'politics',   true,  38.90, -77.03, 'us'),
  ('rss', 'The Hill',              'thehill_rss',      'https://thehill.com/news/feed/',                                1, 0.82, 'MEDIUM', 'politics',   true,  38.90, -77.03, 'us'),
  ('rss', 'Axios',                 'axios_rss',        'https://api.axios.com/feed/',                                   1, 0.88, 'MEDIUM', 'news',       true,  38.90, -77.03, 'us'),
  ('rss', 'The Guardian – World',  'guardian_world',   'https://www.theguardian.com/world/rss',                         1, 0.87, 'MEDIUM', 'news',       true,  51.50,  -0.12, 'gb'),
  ('rss', 'Al Jazeera',            'aljazeera_rss',    'https://www.aljazeera.com/xml/rss/all.xml',                     2, 0.78, 'MEDIUM', 'news',       true,  25.28,  51.53, 'qa'),

-- Tier 2: Markets & Finance
  ('rss', 'MarketWatch – Top',     'marketwatch_rss',  'https://feeds.marketwatch.com/marketwatch/topstories/',         2, 0.82, 'MEDIUM', 'markets',    true,  40.75, -74.01, 'us'),
  ('rss', 'FX Street',             'fxstreet_rss',     'https://www.fxstreet.com/rss/news',                             2, 0.80, 'MEDIUM', 'markets',    true,  40.75, -74.01, 'us'),
  ('rss', 'Investing.com – News',  'investing_rss',    'https://www.investing.com/rss/news_25.rss',                     2, 0.78, 'MEDIUM', 'markets',    true,  40.75, -74.01, 'us'),

-- Tier 2: Crypto
  ('rss', 'CoinDesk',              'coindesk_rss',     'https://www.coindesk.com/arc/outboundfeeds/rss/',               2, 0.88, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us'),
  ('rss', 'The Block',             'theblock_rss',     'https://www.theblock.co/rss/all',                               2, 0.85, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us'),
  ('rss', 'Decrypt',               'decrypt_rss',      'https://decrypt.co/feed',                                       2, 0.82, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us'),
  ('rss', 'Blockworks',            'blockworks_rss',   'https://blockworks.co/feed',                                    2, 0.80, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us'),
  ('rss', 'CoinTelegraph',         'cointelegraph_rss','https://cointelegraph.com/rss',                                  2, 0.78, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us'),

-- Tier 2: Geopolitics / Defense
  ('rss', 'Foreign Policy',        'foreignpol_rss',   'https://foreignpolicy.com/feed/',                               2, 0.85, 'MEDIUM', 'geopolitics',true,  38.90, -77.03, 'us'),
  ('rss', 'Defense One',           'defenseone_rss',   'https://www.defenseone.com/rss/all/',                           2, 0.83, 'MEDIUM', 'osint',      true,  38.90, -77.03, 'us'),
  ('rss', 'Defense News',          'defensenews_rss',  'https://www.defensenews.com/arc/outboundfeeds/rss/',            2, 0.82, 'MEDIUM', 'osint',      true,  38.90, -77.03, 'us'),
  ('rss', 'War on the Rocks',      'warontherocks_rss','https://warontherocks.com/feed/',                               2, 0.82, 'LOW',    'geopolitics',true,  38.90, -77.03, 'us'),

-- Tier 3: International perspective
  ('rss', 'TASS',                  'tass_rss',         'https://tass.com/rss/v2.xml',                                   3, 0.45, 'MEDIUM', 'news',       true,  55.75,  37.62, 'ru'),
  ('rss', 'South China Morning Post','scmp_rss',       'https://www.scmp.com/rss/91/feed',                              3, 0.72, 'MEDIUM', 'news',       true,  22.32, 114.17, 'hk'),

-- CryptoPanic JSON aggregator (handled specially in ingest_cryptopanic)
  ('rss', 'CryptoPanic',           'cryptopanic',      'https://cryptopanic.com/api/v1/posts/?public=true&kind=news',   2, 0.80, 'MEDIUM', 'crypto',     true,  40.75, -74.01, 'us')
ON CONFLICT (source_type, handle) DO UPDATE
  SET feed_url = EXCLUDED.feed_url,
      reliability = EXCLUDED.reliability,
      tier = EXCLUDED.tier,
      updated_at = NOW();
