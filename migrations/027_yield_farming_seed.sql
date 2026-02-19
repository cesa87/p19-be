-- Seed data for yield farming protocols and pools

-- Insert Solana DeFi protocols
INSERT INTO yield_protocols (name, protocol_type, website_url, enabled, risk_score, tvl_usd) VALUES
('Raydium', 'dex', 'https://raydium.io', true, 4, 350000000),
('Orca', 'dex', 'https://orca.so', true, 3, 280000000),
('Kamino Finance', 'lending', 'https://kamino.finance', true, 5, 420000000),
('Jito', 'liquid_staking', 'https://jito.network', true, 2, 1800000000),
('Meteora', 'vault', 'https://meteora.ag', true, 4, 190000000),
('Marinade Finance', 'liquid_staking', 'https://marinade.finance', true, 2, 950000000),
('Drift Protocol', 'derivatives', 'https://drift.trade', true, 6, 150000000),
('Phoenix', 'dex', 'https://phoenix.trade', true, 5, 95000000)
ON CONFLICT (name) DO UPDATE SET
    tvl_usd = EXCLUDED.tvl_usd,
    last_updated = NOW();

-- Insert sample pools for Raydium (DEX)
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active) 
SELECT id, 'SOL-USDC', '58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2', 'SOL', 'USDC', 18.5, 45000000, 'medium', true FROM yield_protocols WHERE name = 'Raydium'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'RAY-SOL', '89ZKE4aoyfLBe2RuV6jM3JGNhaV18Nxh8eNtjRcndBip', 'RAY', 'SOL', 32.7, 12000000, 'high', true FROM yield_protocols WHERE name = 'Raydium'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'USDC-USDT', 'WvZHsq3s9c8fW5HJiM2rM5n6JCRKCvhQcj8VJfLkHdH', 'USDC', 'USDT', 8.2, 28000000, 'low', true FROM yield_protocols WHERE name = 'Raydium'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'BONK-SOL', '3bHrKDKE1bKbBCmPJpnFWpB1RU7EKvzV4J6cPMQWxhfS', 'BONK', 'SOL', 45.3, 8500000, 'high', true FROM yield_protocols WHERE name = 'Raydium'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Insert sample pools for Orca (DEX)
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'SOL-USDC Whirlpool', 'HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ', 'SOL', 'USDC', 22.4, 38000000, 'medium', true FROM yield_protocols WHERE name = 'Orca'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'mSOL-SOL', '9vqYJjDUFecLL2xPUC4Rc7hyCtZ6iJ4mDiVZX7aFXoAe', 'mSOL', 'SOL', 12.8, 25000000, 'low', true FROM yield_protocols WHERE name = 'Orca'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'ORCA-USDC', 'AioST8HKQJRqjE1mknk4Rydc8wVADhdQwRJmAAYX1T6Z', 'ORCA', 'USDC', 28.9, 9200000, 'high', true FROM yield_protocols WHERE name = 'Orca'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'jitoSOL-SOL', '3ne4mWqdYuNiYrYZC9TrA3FcfuFdErghH97vNPbjicr1', 'jitoSOL', 'SOL', 9.3, 42000000, 'low', true FROM yield_protocols WHERE name = 'Orca'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Insert sample pools for Kamino (Lending)
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'USDC Lending', 'FpBAp3hVo5FXJhKYa4FGjuNWFxqVXALvj5N5Jpk4CU7W', 'USDC', NULL, 14.2, 180000000, 'none', true FROM yield_protocols WHERE name = 'Kamino Finance'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'SOL Lending', 'ELdGFWnGqNBxDuP5Z7VJwxQXPW7mMfJPjBcvqCJqTNGQ', 'SOL', NULL, 11.7, 95000000, 'none', true FROM yield_protocols WHERE name = 'Kamino Finance'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'jitoSOL Lending', 'BZ8VxfAuwGSJNzJVLf8D7LJmtBMZjpQELaRcgKdwRk6R', 'jitoSOL', NULL, 8.9, 62000000, 'none', true FROM yield_protocols WHERE name = 'Kamino Finance'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'mSOL Lending', '4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8', 'mSOL', NULL, 7.8, 48000000, 'none', true FROM yield_protocols WHERE name = 'Kamino Finance'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Insert Jito staking pool
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'Jito Staking', 'Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb', 'SOL', 'jitoSOL', 7.2, 1800000000, 'none', true FROM yield_protocols WHERE name = 'Jito'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Insert Marinade staking pool
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'Marinade Staking', 'MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD', 'SOL', 'mSOL', 6.8, 950000000, 'none', true FROM yield_protocols WHERE name = 'Marinade Finance'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Insert Meteora vaults
INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'SOL-USDC Vault', 'DLMM1YFJmpvv7dYXQmjcXsBHtvFMZDH9QnxLbWxDd5bG', 'SOL', 'USDC', 25.6, 55000000, 'medium', true FROM yield_protocols WHERE name = 'Meteora'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

INSERT INTO yield_pools (protocol_id, pool_name, pool_address, token_a, token_b, current_apy, tvl_usd, impermanent_loss_risk, active)
SELECT id, 'jitoSOL-SOL Vault', 'DLMM2sJKnqjPBmNfJpYUzBHNVN73tqEqYxAHKLqbSn4V', 'jitoSOL', 'SOL', 10.4, 38000000, 'low', true FROM yield_protocols WHERE name = 'Meteora'
ON CONFLICT (pool_address) DO UPDATE SET current_apy = EXCLUDED.current_apy, tvl_usd = EXCLUDED.tvl_usd, last_updated = NOW();

-- Update APY history for some pools (7-day and 30-day averages)
UPDATE yield_pools SET
    apy_7d_avg = current_apy * 0.95,
    apy_30d_avg = current_apy * 0.92
WHERE pool_address IN ('58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2', 'HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ', 'FpBAp3hVo5FXJhKYa4FGjuNWFxqVXALvj5N5Jpk4CU7W');

UPDATE yield_pools SET
    apy_7d_avg = current_apy * 1.1,
    apy_30d_avg = current_apy * 0.88
WHERE pool_address IN ('89ZKE4aoyfLBe2RuV6jM3JGNhaV18Nxh8eNtjRcndBip', '3bHrKDKE1bKbBCmPJpnFWpB1RU7EKvzV4J6cPMQWxhfS', 'AioST8HKQJRqjE1mknk4Rydc8wVADhdQwRJmAAYX1T6Z');

-- Add some protocol events for context  
INSERT INTO protocol_events (protocol_id, event_type, severity, title, description, event_at)
SELECT id, 'security_audit', 'info', 'Security Audit Passed', 'Completed comprehensive security audit by OtterSec with no critical findings', NOW() - INTERVAL '30 days'
FROM yield_protocols WHERE name = 'Jito';

INSERT INTO protocol_events (protocol_id, event_type, severity, title, description, event_at)
SELECT id, 'upgrade', 'info', 'Protocol Upgrade v2.0', 'Upgraded to v2.0 with improved capital efficiency and lower fees', NOW() - INTERVAL '15 days'
FROM yield_protocols WHERE name = 'Kamino Finance';

INSERT INTO protocol_events (protocol_id, event_type, severity, title, description, event_at)
SELECT id, 'tvl_milestone', 'info', 'TVL Milestone', 'Total Value Locked surpassed $1B', NOW() - INTERVAL '7 days'
FROM yield_protocols WHERE name = 'Jito';
