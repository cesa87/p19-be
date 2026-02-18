-- Bots table - configurable trading bot instances
CREATE TABLE IF NOT EXISTS bots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    
    -- Strategy
    strategy_id UUID REFERENCES strategies(id) ON DELETE SET NULL,
    
    -- Broker configuration
    broker VARCHAR(50) NOT NULL DEFAULT 'oanda',
    account_type VARCHAR(20) NOT NULL DEFAULT 'demo', -- 'demo' or 'live'
    api_token TEXT,  -- Encrypted in production
    account_id VARCHAR(100),
    
    -- Trading parameters
    lot_size DECIMAL(10, 4) NOT NULL DEFAULT 0.01,
    max_positions INT NOT NULL DEFAULT 1,
    max_daily_trades INT DEFAULT 10,
    max_daily_loss_pct DECIMAL(5, 2) DEFAULT 5.0,
    
    -- Risk management
    use_strategy_sl_tp BOOLEAN DEFAULT true,  -- Use strategy's SL/TP or override
    override_sl_pips DECIMAL(10, 2),
    override_tp_pips DECIMAL(10, 2),
    
    -- Scheduling
    trading_hours_start TIME,  -- NULL = 24/7
    trading_hours_end TIME,
    trading_days VARCHAR(20) DEFAULT 'mon,tue,wed,thu,fri',
    
    -- Status
    is_active BOOLEAN NOT NULL DEFAULT false,
    last_started_at TIMESTAMP WITH TIME ZONE,
    last_stopped_at TIMESTAMP WITH TIME ZONE,
    
    -- Stats
    total_trades INT DEFAULT 0,
    total_pnl DECIMAL(20, 2) DEFAULT 0,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bots_user_id ON bots(user_id);
CREATE INDEX idx_bots_is_active ON bots(is_active);
