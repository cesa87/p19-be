-- Trade Context: Store market conditions at trade entry for learning/adaptation
-- This enables performance analysis by condition (RSI level, regime, time, etc.)

CREATE TABLE IF NOT EXISTS trade_context (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Link to trade (nullable until we have internal trade tracking)
    external_trade_id VARCHAR(50),  -- OANDA trade ID
    bot_id UUID REFERENCES bots(id) ON DELETE CASCADE,
    strategy_id UUID REFERENCES strategies(id) ON DELETE SET NULL,
    
    -- Trade basics
    symbol VARCHAR(20) NOT NULL,
    direction VARCHAR(10) NOT NULL CHECK (direction IN ('LONG', 'SHORT')),
    entry_price DECIMAL(12, 4) NOT NULL,
    lot_size DECIMAL(12, 4) NOT NULL,
    
    -- Technical indicators at entry
    rsi_at_entry DECIMAL(5, 2),
    adx_at_entry DECIMAL(5, 2),
    atr_at_entry DECIMAL(12, 4),
    ema_trend_at_entry DECIMAL(12, 4),      -- e.g., EMA200 value
    ema_entry_at_entry DECIMAL(12, 4),      -- e.g., EMA21 value
    price_vs_ema_pct DECIMAL(8, 4),         -- % distance from trend EMA
    volatility_ratio DECIMAL(5, 2),          -- Current ATR / Avg ATR
    
    -- MRATE context at entry
    mrate_regime VARCHAR(30),
    mrate_favored_instrument VARCHAR(20),
    mrate_trend_weight DECIMAL(5, 4),
    mrate_risk_multiplier DECIMAL(5, 4),
    mrate_liquidity_score DECIMAL(5, 2),
    mrate_uncertainty_score DECIMAL(5, 2),
    
    -- Time context
    entry_hour INTEGER NOT NULL,             -- 0-23 UTC
    entry_day_of_week INTEGER NOT NULL,      -- 0=Sunday, 6=Saturday
    trading_session VARCHAR(20),             -- 'ASIAN', 'LONDON', 'NEW_YORK', 'OVERLAP'
    
    -- Price action context
    recent_move_atr DECIMAL(5, 2),           -- Move in last N candles / ATR
    candles_since_last_signal INTEGER,
    
    -- Signal details
    signal_confidence DECIMAL(5, 4),
    signal_reason TEXT,
    
    -- Strategy parameters used (for tracking what settings produced this trade)
    strategy_params JSONB,
    
    -- Outcome (filled in when trade closes)
    exit_price DECIMAL(12, 4),
    pnl DECIMAL(12, 4),
    pnl_atr DECIMAL(8, 4),                   -- P&L in ATR units (normalized)
    trade_duration_minutes INTEGER,
    exit_reason VARCHAR(50),                  -- 'TP_HIT', 'SL_HIT', 'MANUAL', 'SIGNAL_REVERSE'
    is_winner BOOLEAN,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMPTZ
);

-- Indexes for analytics queries
CREATE INDEX IF NOT EXISTS idx_trade_context_bot_id ON trade_context(bot_id);
CREATE INDEX IF NOT EXISTS idx_trade_context_strategy_id ON trade_context(strategy_id);
CREATE INDEX IF NOT EXISTS idx_trade_context_symbol ON trade_context(symbol);
CREATE INDEX IF NOT EXISTS idx_trade_context_created_at ON trade_context(created_at);
CREATE INDEX IF NOT EXISTS idx_trade_context_mrate_regime ON trade_context(mrate_regime);
CREATE INDEX IF NOT EXISTS idx_trade_context_rsi ON trade_context(rsi_at_entry);
CREATE INDEX IF NOT EXISTS idx_trade_context_session ON trade_context(trading_session);
CREATE INDEX IF NOT EXISTS idx_trade_context_hour ON trade_context(entry_hour);
CREATE INDEX IF NOT EXISTS idx_trade_context_winner ON trade_context(is_winner);

-- Strategy adaptations table (for Phase 3 - auto parameter adjustment)
CREATE TABLE IF NOT EXISTS strategy_adaptations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id UUID REFERENCES strategies(id) ON DELETE CASCADE,
    bot_id UUID REFERENCES bots(id) ON DELETE CASCADE,
    
    -- What's being adapted
    parameter_name VARCHAR(50) NOT NULL,
    original_value DECIMAL(12, 4) NOT NULL,
    adapted_value DECIMAL(12, 4) NOT NULL,
    
    -- Why
    reason TEXT NOT NULL,
    condition_analyzed VARCHAR(100),         -- e.g., 'rsi_at_entry < 35'
    
    -- Evidence
    trades_analyzed INTEGER NOT NULL,
    original_win_rate DECIMAL(5, 4),
    adapted_win_rate DECIMAL(5, 4),
    confidence DECIMAL(5, 4),
    
    -- Status
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'APPROVED', 'REJECTED', 'EXPIRED', 'AUTO_APPLIED')),
    approved_by VARCHAR(50),                  -- 'AUTO' or user
    
    -- Validity
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_strategy_adaptations_strategy ON strategy_adaptations(strategy_id);
CREATE INDEX IF NOT EXISTS idx_strategy_adaptations_status ON strategy_adaptations(status);
