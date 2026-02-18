-- Test data generator for Risk Dashboard
-- Run this to populate the dashboard with sample data

-- This assumes you have at least one bot and strategy in the database
-- Adjust the UUIDs below to match your actual bot_id and strategy_id

-- Get a bot_id and strategy_id (replace these with actual values from your DB)
-- SELECT id FROM bots LIMIT 1;
-- SELECT id FROM strategies LIMIT 1;

-- Example: Insert some sample closed trades to test correlation
-- Replace 'YOUR_BOT_ID' and 'YOUR_STRATEGY_ID' with actual UUIDs

-- Sample trade data for Strategy 1 (Trend Following)
INSERT INTO trade_context (
    bot_id,
    strategy_id,
    external_trade_id,
    symbol,
    direction,
    entry_price,
    lot_size,
    pnl,
    is_winner,
    closed_at,
    created_at
) VALUES
-- Winning trades
('YOUR_BOT_ID', 'YOUR_STRATEGY_ID', 'TEST-001', 'XAU_USD', 'LONG', 2040.50, 0.1, 150.00, true, NOW() - INTERVAL '1 day', NOW() - INTERVAL '2 days'),
('YOUR_BOT_ID', 'YOUR_STRATEGY_ID', 'TEST-002', 'XAU_USD', 'LONG', 2035.20, 0.1, 200.00, true, NOW() - INTERVAL '2 days', NOW() - INTERVAL '3 days'),
('YOUR_BOT_ID', 'YOUR_STRATEGY_ID', 'TEST-003', 'XAU_USD', 'SHORT', 2050.00, 0.1, 180.00, true, NOW() - INTERVAL '3 days', NOW() - INTERVAL '4 days'),
-- Losing trades
('YOUR_BOT_ID', 'YOUR_STRATEGY_ID', 'TEST-004', 'XAU_USD', 'LONG', 2045.00, 0.1, -120.00, false, NOW() - INTERVAL '4 days', NOW() - INTERVAL '5 days'),
('YOUR_BOT_ID', 'YOUR_STRATEGY_ID', 'TEST-005', 'XAU_USD', 'SHORT', 2038.50, 0.1, -90.00, false, NOW() - INTERVAL '5 days', NOW() - INTERVAL '6 days');

-- To actually see data in the dashboard, you need to:
-- 1. Have a running bot (the risk engine tracks live positions)
-- 2. The bot should have open positions or recent trades
-- 3. Account equity updates from the broker every 5 minutes

-- Quick check queries:
-- SELECT * FROM bots;
-- SELECT * FROM strategies;
-- SELECT * FROM trade_context ORDER BY created_at DESC LIMIT 10;
