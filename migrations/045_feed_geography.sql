-- Add geography to feed sources and posts

ALTER TABLE feed_sources
    ADD COLUMN IF NOT EXISTS latitude     DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude    DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS country_code VARCHAR(5);

ALTER TABLE feed_posts
    ADD COLUMN IF NOT EXISTS latitude  DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;

-- Update existing sources with geographic home
UPDATE feed_sources SET latitude = 40.7,  longitude = -74.0,  country_code = 'US' WHERE handle = 'walter_bloomberg';
UPDATE feed_sources SET latitude = 40.7,  longitude = -74.0,  country_code = 'US' WHERE handle = 'polymarketupdates';
UPDATE feed_sources SET latitude = 40.7,  longitude = -74.0,  country_code = 'US' WHERE handle = 'marketsmayhem';
UPDATE feed_sources SET latitude = 25.8,  longitude = -80.2,  country_code = 'US' WHERE handle = 'cryptowhalealerts';
UPDATE feed_sources SET latitude = 37.4,  longitude = -122.1, country_code = 'US' WHERE handle = 'realvisionfinance';
UPDATE feed_sources SET latitude = 51.5,  longitude = -0.1,   country_code = 'GB' WHERE handle = 'macroalf';

-- Seed global Telegram channels with geographic home coordinates
INSERT INTO feed_sources (source_type, name, handle, default_severity, latitude, longitude, country_code) VALUES
-- Eastern Europe / Conflict
('telegram', 'Intel Slava Z',          'intelslava',           'HIGH',     48.5,   32.0,   'UA'),
('telegram', 'NEXTA Live',             'nexta_live',           'HIGH',     53.9,   27.5,   'BY'),
('telegram', 'Ukraine War Map',        'ukrainewarmap',        'HIGH',     49.0,   31.5,   'UA'),
('telegram', 'Military Summary',       'militarysummary',      'HIGH',     50.4,   30.5,   'UA'),
-- Middle East
('telegram', 'War Monitor',            'WarMonitor',           'HIGH',     31.8,   35.2,   'IL'),
('telegram', 'Arabian Gulf Monitor',   'arabgulfmonitor',      'HIGH',     24.0,   45.0,   'SA'),
('telegram', 'Palestine Chronicle',    'PalestineChronicle',   'HIGH',     31.9,   35.2,   'PS'),
-- UK / Europe
('telegram', 'The Signal Centre',      'thesignalcentre',      'MEDIUM',   51.5,   -0.1,   'GB'),
('telegram', 'Euro Macro Watch',       'euromacrowatch',       'MEDIUM',   50.1,    8.7,   'DE'),
('telegram', 'Geopolitics Live',       'geopolitics_live',     'HIGH',     48.9,    2.3,   'FR'),
-- Russia
('telegram', 'Rybar',                  'rybar_en',             'HIGH',     55.7,   37.6,   'RU'),
-- Asia
('telegram', 'Asia Markets',           'asia_markets_news',    'MEDIUM',    1.3,  103.8,   'SG'),
('telegram', 'India Economic Times',   'EconomicTimesApp',     'MEDIUM',   28.6,   77.2,   'IN'),
('telegram', 'South China Morning',    'scmpnews',             'MEDIUM',   22.3,  114.1,   'HK'),
-- Americas
('telegram', 'Otavio Costa Macro',     'otaviocosta_',         'HIGH',    -23.5,  -46.6,   'BR'),
('telegram', 'LatAm Daily',            'latam_daily',          'MEDIUM',  -33.4,  -70.6,   'CL'),
-- Africa / Global
('telegram', 'Africa Intelligence',    'africaintelligence',   'MEDIUM',   -1.3,   36.8,   'KE'),
('telegram', 'Commodity Watch',        'commoditywatch',       'MEDIUM',  -26.2,   28.0,   'ZA')
ON CONFLICT (source_type, handle) DO NOTHING;
