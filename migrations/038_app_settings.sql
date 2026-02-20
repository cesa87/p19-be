-- App-wide settings (key/value store)
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- MRATE starts DISABLED so bots trade freely until user enables it
INSERT INTO app_settings (key, value) VALUES ('mrate_enabled', 'false')
ON CONFLICT (key) DO NOTHING;
