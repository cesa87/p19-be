-- ═══════════════════════════════════════════════════════════════════════════
-- Yield Farming Schema - Solana DeFi Protocol Integration
-- ═══════════════════════════════════════════════════════════════════════════

-- 1. Supported Protocols
-- Track which Solana DeFi protocols we integrate with
CREATE TABLE IF NOT EXISTS yield_protocols (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE, -- e.g. "Raydium", "Orca", "Kamino"
    protocol_type VARCHAR(50) NOT NULL, -- "dex", "lending", "liquid_staking", "vault"
    enabled BOOLEAN NOT NULL DEFAULT true,
    risk_score INTEGER NOT NULL DEFAULT 5, -- 1-10 scale (1=safest, 10=highest risk)
    tvl_usd NUMERIC(20, 2), -- Total Value Locked
    -- API/SDK configuration
    api_endpoint TEXT,
    requires_auth BOOLEAN DEFAULT false,
    -- Metadata
    website_url TEXT,
    docs_url TEXT,
    audit_report_url TEXT,
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_yield_protocols_enabled ON yield_protocols(enabled) WHERE enabled = true;
CREATE INDEX idx_yield_protocols_type ON yield_protocols(protocol_type);

-- 2. Yield Pools/Vaults
-- Individual yield opportunities within protocols
CREATE TABLE IF NOT EXISTS yield_pools (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    protocol_id UUID NOT NULL REFERENCES yield_protocols(id) ON DELETE CASCADE,
    
    -- Pool identification
    pool_address VARCHAR(100) NOT NULL, -- Solana public key
    pool_name VARCHAR(200) NOT NULL, -- e.g. "USDC-USDT LP", "jitoSOL Vault"
    
    -- Assets
    token_a VARCHAR(20) NOT NULL, -- e.g. "USDC"
    token_b VARCHAR(20), -- NULL for single-asset pools
    lp_token_mint VARCHAR(100), -- LP token mint address
    
    -- Yield metrics
    current_apy NUMERIC(8, 4) NOT NULL, -- e.g. 35.5000 for 35.5%
    apy_7d_avg NUMERIC(8, 4), -- 7-day average APY
    apy_30d_avg NUMERIC(8, 4), -- 30-day average APY
    
    -- Pool health
    tvl_usd NUMERIC(20, 2) NOT NULL,
    volume_24h_usd NUMERIC(20, 2),
    fees_24h_usd NUMERIC(20, 2),
    
    -- Risk indicators
    impermanent_loss_risk VARCHAR(20) DEFAULT 'medium', -- "low", "medium", "high"
    liquidity_depth_score INTEGER DEFAULT 5, -- 1-10 (10=deepest)
    
    -- Status
    active BOOLEAN NOT NULL DEFAULT true,
    is_verified BOOLEAN DEFAULT false, -- Protocol officially verified
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_yield_pools_address ON yield_pools(pool_address);
CREATE INDEX idx_yield_pools_protocol ON yield_pools(protocol_id);
CREATE INDEX idx_yield_pools_active ON yield_pools(active) WHERE active = true;
CREATE INDEX idx_yield_pools_apy ON yield_pools(current_apy DESC) WHERE active = true;
CREATE INDEX idx_yield_pools_tokens ON yield_pools(token_a, token_b);

-- 3. Active Positions
-- User's current yield farming positions
CREATE TABLE IF NOT EXISTS yield_positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pool_id UUID NOT NULL REFERENCES yield_pools(id),
    
    -- Position details
    wallet_address VARCHAR(100) NOT NULL, -- User's Solana wallet
    deposit_amount_usd NUMERIC(20, 2) NOT NULL,
    lp_tokens_amount NUMERIC(30, 10) NOT NULL, -- LP tokens received
    
    -- Entry metrics
    entry_apy NUMERIC(8, 4) NOT NULL, -- APY at time of entry
    entry_token_a_price NUMERIC(20, 8),
    entry_token_b_price NUMERIC(20, 8),
    
    -- Current value
    current_value_usd NUMERIC(20, 2),
    unrealized_pnl_usd NUMERIC(20, 2),
    rewards_earned_usd NUMERIC(20, 2) DEFAULT 0,
    
    -- Harvest tracking
    last_harvest_at TIMESTAMPTZ,
    harvest_count INTEGER DEFAULT 0,
    auto_compound BOOLEAN DEFAULT true,
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'active', -- "active", "pending_exit", "exited"
    
    -- Timestamps
    entered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    exited_at TIMESTAMPTZ,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_yield_positions_wallet ON yield_positions(wallet_address);
CREATE INDEX idx_yield_positions_pool ON yield_positions(pool_id);
CREATE INDEX idx_yield_positions_status ON yield_positions(status);
CREATE INDEX idx_yield_positions_active ON yield_positions(status) WHERE status = 'active';

-- 4. Rotation History
-- Track when and why we move funds between pools
CREATE TABLE IF NOT EXISTS yield_rotations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Source & Destination
    from_pool_id UUID REFERENCES yield_pools(id),
    to_pool_id UUID NOT NULL REFERENCES yield_pools(id),
    
    -- Amounts
    amount_usd NUMERIC(20, 2) NOT NULL,
    from_apy NUMERIC(8, 4),
    to_apy NUMERIC(8, 4),
    
    -- Reason & Strategy
    rotation_reason VARCHAR(50) NOT NULL, -- "higher_yield", "risk_reduction", "regime_change", "manual"
    apy_improvement NUMERIC(8, 4), -- Percentage points improvement
    
    -- Transaction details
    wallet_address VARCHAR(100) NOT NULL,
    transaction_signature VARCHAR(200), -- Solana tx signature
    gas_cost_sol NUMERIC(20, 10),
    
    -- AI Decision factors
    regime_at_rotation VARCHAR(50), -- MRATE regime
    risk_score_change INTEGER, -- Change in overall risk
    ai_confidence NUMERIC(4, 3), -- 0.0-1.0
    
    -- Result tracking
    success BOOLEAN,
    error_message TEXT,
    
    -- Timestamps
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_yield_rotations_wallet ON yield_rotations(wallet_address);
CREATE INDEX idx_yield_rotations_from_pool ON yield_rotations(from_pool_id);
CREATE INDEX idx_yield_rotations_to_pool ON yield_rotations(to_pool_id);
CREATE INDEX idx_yield_rotations_executed ON yield_rotations(executed_at DESC);

-- 5. Yield History
-- Time-series data for APY tracking and analysis
CREATE TABLE IF NOT EXISTS yield_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pool_id UUID NOT NULL REFERENCES yield_pools(id) ON DELETE CASCADE,
    
    -- Snapshot metrics
    apy NUMERIC(8, 4) NOT NULL,
    tvl_usd NUMERIC(20, 2) NOT NULL,
    volume_24h_usd NUMERIC(20, 2),
    
    -- Timestamp
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_yield_history_pool ON yield_history(pool_id, recorded_at DESC);
CREATE INDEX idx_yield_history_time ON yield_history(recorded_at DESC);

-- 6. Protocol Health Events
-- Track important protocol events (audits, hacks, upgrades)
CREATE TABLE IF NOT EXISTS protocol_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    protocol_id UUID NOT NULL REFERENCES yield_protocols(id),
    
    -- Event details
    event_type VARCHAR(50) NOT NULL, -- "audit", "hack", "upgrade", "pause", "resume"
    severity VARCHAR(20) NOT NULL, -- "info", "warning", "critical"
    title VARCHAR(200) NOT NULL,
    description TEXT,
    
    -- Impact
    funds_affected_usd NUMERIC(20, 2),
    recommended_action VARCHAR(50), -- "continue", "reduce_exposure", "exit_immediately"
    
    -- Source
    source_url TEXT,
    
    -- Timestamp
    event_at TIMESTAMPTZ NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_protocol_events_protocol ON protocol_events(protocol_id, event_at DESC);
CREATE INDEX idx_protocol_events_severity ON protocol_events(severity) WHERE severity IN ('warning', 'critical');

-- ═══════════════════════════════════════════════════════════════════════════
-- Views for Analytics
-- ═══════════════════════════════════════════════════════════════════════════

-- Top performing pools
CREATE OR REPLACE VIEW top_yield_pools AS
SELECT 
    yp.id,
    yp.pool_name,
    yp.token_a,
    yp.token_b,
    yp.current_apy,
    yp.apy_7d_avg,
    yp.tvl_usd,
    yp.volume_24h_usd,
    prot.name as protocol_name,
    prot.risk_score as protocol_risk_score
FROM yield_pools yp
JOIN yield_protocols prot ON prot.id = yp.protocol_id
WHERE yp.active = true 
  AND prot.enabled = true
  AND yp.tvl_usd > 100000 -- Minimum $100k TVL
ORDER BY yp.current_apy DESC
LIMIT 50;

-- Portfolio summary
CREATE OR REPLACE VIEW yield_portfolio_summary AS
SELECT 
    pos.wallet_address,
    COUNT(DISTINCT pos.pool_id) as active_positions,
    SUM(pos.deposit_amount_usd) as total_deposited_usd,
    SUM(pos.current_value_usd) as total_current_value_usd,
    SUM(pos.unrealized_pnl_usd) as total_unrealized_pnl_usd,
    SUM(pos.rewards_earned_usd) as total_rewards_earned_usd,
    AVG(yp.current_apy) as weighted_avg_apy
FROM yield_positions pos
JOIN yield_pools yp ON yp.id = pos.pool_id
WHERE pos.status = 'active'
GROUP BY pos.wallet_address;

-- Recent rotations with outcome
CREATE OR REPLACE VIEW recent_rotations AS
SELECT 
    yr.id,
    yr.executed_at,
    from_pool.pool_name as from_pool_name,
    to_pool.pool_name as to_pool_name,
    yr.amount_usd,
    yr.from_apy,
    yr.to_apy,
    yr.apy_improvement,
    yr.rotation_reason,
    yr.success,
    yr.ai_confidence
FROM yield_rotations yr
LEFT JOIN yield_pools from_pool ON from_pool.id = yr.from_pool_id
JOIN yield_pools to_pool ON to_pool.id = yr.to_pool_id
ORDER BY yr.executed_at DESC
LIMIT 100;

-- Comments
COMMENT ON TABLE yield_protocols IS 'Solana DeFi protocols (Raydium, Orca, Kamino, Jito, Meteora)';
COMMENT ON TABLE yield_pools IS 'Individual yield opportunities (LP pools, vaults, staking)';
COMMENT ON TABLE yield_positions IS 'Active user positions in yield pools';
COMMENT ON TABLE yield_rotations IS 'History of fund movements between pools';
COMMENT ON TABLE yield_history IS 'Time-series APY tracking for analysis';
COMMENT ON TABLE protocol_events IS 'Security/operational events affecting protocols';
