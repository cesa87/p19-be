-- Performance index for bot cooldown queries
-- The cooldown endpoint queries: WHERE bot_id = $1 AND activity_type = 'order_placed' ORDER BY created_at DESC
-- Without this composite index, the query does a full table scan

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_bot_activities_cooldown 
ON bot_activities(bot_id, activity_type, created_at DESC);

-- This index covers the exact query pattern and can:
-- 1. Filter by bot_id (first column)
-- 2. Filter by activity_type (second column)
-- 3. Sort by created_at DESC (included in index)
-- 4. Return created_at directly from index (no table lookup needed)
