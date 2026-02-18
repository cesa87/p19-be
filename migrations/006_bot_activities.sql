-- Bot activities log table
-- Tracks what bots are doing in real-time

CREATE TABLE IF NOT EXISTS bot_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bot_id UUID NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    activity_type VARCHAR(50) NOT NULL,
    message TEXT NOT NULL,
    details JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast lookups by bot
CREATE INDEX IF NOT EXISTS idx_bot_activities_bot_id ON bot_activities(bot_id);

-- Index for recent activities
CREATE INDEX IF NOT EXISTS idx_bot_activities_created_at ON bot_activities(created_at DESC);

-- Cleanup old activities (keep last 1000 per bot)
-- This can be run periodically
-- DELETE FROM bot_activities WHERE id NOT IN (
--     SELECT id FROM bot_activities ba2 
--     WHERE ba2.bot_id = bot_activities.bot_id 
--     ORDER BY created_at DESC LIMIT 1000
-- );
