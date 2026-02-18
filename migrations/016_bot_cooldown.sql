-- Add cooldown_seconds to bots table for per-bot configuration
-- This overrides the strategy default when set

ALTER TABLE bots 
ADD COLUMN IF NOT EXISTS cooldown_seconds INTEGER DEFAULT NULL;

-- NULL means use strategy default
-- Setting a value overrides the strategy default

COMMENT ON COLUMN bots.cooldown_seconds IS 'Trade cooldown in seconds. NULL = use strategy default.';
