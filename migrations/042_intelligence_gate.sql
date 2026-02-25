-- Per-bot intelligence gate toggle (default OFF — no impact until manually enabled)
ALTER TABLE bots ADD COLUMN IF NOT EXISTS intelligence_gate_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- Polymarket USDC trading limits (dynamic settings)
INSERT INTO app_settings (key, value) VALUES ('polymarket_max_trade_usdc', '50')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value) VALUES ('polymarket_max_total_usdc', '5000')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value) VALUES ('polymarket_trading_enabled', 'false')
ON CONFLICT (key) DO NOTHING;

-- Intelligence gate thresholds (global defaults, can be overridden)
INSERT INTO app_settings (key, value) VALUES ('intel_gate_block_tension', '75')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value) VALUES ('intel_gate_reduce_tension', '60')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value) VALUES ('intel_gate_boost_opportunity', '70')
ON CONFLICT (key) DO NOTHING;

INSERT INTO app_settings (key, value) VALUES ('intel_gate_min_confidence', '0.3')
ON CONFLICT (key) DO NOTHING;
