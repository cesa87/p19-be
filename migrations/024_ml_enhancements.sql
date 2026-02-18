-- ═══════════════════════════════════════════════════════════════════════════
-- ML Enhancements Schema
-- ═══════════════════════════════════════════════════════════════════════════

-- 1. Cross-Bot Ensemble Learning
-- Tracks collective learning across bot clusters
CREATE TABLE IF NOT EXISTS cross_bot_learnings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cluster_id VARCHAR(100) NOT NULL, -- e.g. "trend_XAU_USD", "mean_reversion_BTC_USD"
    pattern_hash VARCHAR(64) NOT NULL, -- Hash of market conditions (regime, session, RSI zone, etc)
    regime VARCHAR(50) NOT NULL,
    session VARCHAR(50), -- e.g. "London", "NY", "Asian"
    outcome VARCHAR(20) NOT NULL, -- "win", "loss", "breakeven"
    confidence REAL NOT NULL, -- Signal confidence that produced this trade
    pnl REAL NOT NULL,
    trade_id UUID,
    bot_id UUID NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cross_bot_learnings_cluster ON cross_bot_learnings(cluster_id, created_at DESC);
CREATE INDEX idx_cross_bot_learnings_pattern ON cross_bot_learnings(cluster_id, pattern_hash, created_at DESC);

-- 2. Regime Transition History
-- Store MRATE snapshots for training regime prediction model
CREATE TABLE IF NOT EXISTS regime_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMP NOT NULL,
    regime VARCHAR(50) NOT NULL,
    liquidity_score REAL NOT NULL,
    risk_score REAL NOT NULL,
    uncertainty_score REAL NOT NULL,
    liquidity_momentum REAL NOT NULL,
    risk_momentum REAL NOT NULL,
    -- Features for prediction
    vix_level REAL,
    dxy_level REAL,
    fear_greed_index REAL,
    btc_trend VARCHAR(20),
    sp500_trend VARCHAR(20),
    -- Next regime (for supervised learning)
    next_regime VARCHAR(50),
    next_regime_timestamp TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_regime_snapshots_timestamp ON regime_snapshots(timestamp DESC);
CREATE INDEX idx_regime_snapshots_regime ON regime_snapshots(regime, timestamp DESC);

-- 3. Bot Variants (Parameter Evolution)
-- Track A/B test variants and their performance
CREATE TABLE IF NOT EXISTS bot_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_bot_id UUID NOT NULL,
    variant_name VARCHAR(100) NOT NULL,
    generation INTEGER NOT NULL DEFAULT 1,
    status VARCHAR(20) NOT NULL DEFAULT 'testing', -- testing, promoted, retired
    -- Parameter mutations
    params JSONB NOT NULL, -- Stores variant parameters (RSI thresholds, ATR multiples, etc)
    -- Performance tracking
    trades_count INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    total_pnl REAL NOT NULL DEFAULT 0,
    win_rate REAL,
    profit_factor REAL,
    sharpe_ratio REAL,
    -- Lifecycle
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    promoted_at TIMESTAMP,
    retired_at TIMESTAMP
);

CREATE INDEX idx_bot_variants_parent ON bot_variants(parent_bot_id, status);
CREATE INDEX idx_bot_variants_performance ON bot_variants(win_rate DESC, profit_factor DESC) WHERE status = 'testing';

-- 4. Instrument Correlations
-- Pre-computed correlation matrix for risk management
CREATE TABLE IF NOT EXISTS instrument_correlations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instrument_a VARCHAR(20) NOT NULL,
    instrument_b VARCHAR(20) NOT NULL,
    correlation REAL NOT NULL, -- Pearson correlation (-1.0 to 1.0)
    window_days INTEGER NOT NULL DEFAULT 30,
    sample_size INTEGER NOT NULL,
    last_updated TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_correlations_pair ON instrument_correlations(instrument_a, instrument_b);
CREATE INDEX idx_correlations_strength ON instrument_correlations(ABS(correlation) DESC);

-- 5. Event Impact Learning
-- Learn which economic events actually move markets
CREATE TABLE IF NOT EXISTS event_impacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_name VARCHAR(200) NOT NULL,
    event_category VARCHAR(50), -- e.g. "Employment", "Inflation", "Central Bank"
    -- Learned impact metrics
    avg_volatility_spike REAL NOT NULL, -- Average ATR spike in 30min after event
    max_volatility_spike REAL,
    avg_price_move_pct REAL, -- Average % price move
    sample_size INTEGER NOT NULL DEFAULT 0,
    impact_score INTEGER NOT NULL DEFAULT 5, -- 0-10 scale
    -- Metadata
    last_occurrence TIMESTAMP,
    last_updated TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_event_impacts_name ON event_impacts(event_name);
CREATE INDEX idx_event_impacts_score ON event_impacts(impact_score DESC);

-- 6. Sentiment Signals
-- Track sentiment-driven confidence adjustments
CREATE TABLE IF NOT EXISTS sentiment_signals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMP NOT NULL DEFAULT NOW(),
    -- Source sentiment
    fear_greed_index REAL,
    reddit_crypto REAL,
    reddit_btc REAL,
    reddit_gold REAL,
    -- Derived signals
    contrarian_signal VARCHAR(20), -- "extreme_fear", "extreme_greed", "neutral"
    confidence_multiplier REAL NOT NULL, -- Applied to bot signals
    -- Context
    regime VARCHAR(50) NOT NULL,
    active_bots_count INTEGER NOT NULL,
    trades_influenced INTEGER DEFAULT 0
);

CREATE INDEX idx_sentiment_signals_timestamp ON sentiment_signals(timestamp DESC);

-- ═══════════════════════════════════════════════════════════════════════════
-- Views for Analytics
-- ═══════════════════════════════════════════════════════════════════════════

-- Cluster performance summary
CREATE OR REPLACE VIEW cluster_performance AS
SELECT 
    cluster_id,
    regime,
    COUNT(*) as total_trades,
    SUM(CASE WHEN outcome = 'win' THEN 1 ELSE 0 END) as wins,
    SUM(CASE WHEN outcome = 'loss' THEN 1 ELSE 0 END) as losses,
    AVG(CASE WHEN outcome = 'win' THEN 1.0 ELSE 0.0 END) as win_rate,
    AVG(pnl) as avg_pnl,
    SUM(pnl) as total_pnl
FROM cross_bot_learnings
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY cluster_id, regime;

-- Recent regime transitions
CREATE OR REPLACE VIEW regime_transitions AS
SELECT 
    r1.timestamp as transition_time,
    r1.regime as from_regime,
    r2.regime as to_regime,
    r2.timestamp - r1.timestamp as duration,
    r1.liquidity_momentum,
    r1.risk_momentum
FROM regime_snapshots r1
JOIN regime_snapshots r2 ON r2.id = (
    SELECT id FROM regime_snapshots 
    WHERE timestamp > r1.timestamp AND regime != r1.regime
    ORDER BY timestamp ASC LIMIT 1
)
WHERE r1.timestamp > NOW() - INTERVAL '7 days'
ORDER BY r1.timestamp DESC;

-- Top performing variants
CREATE OR REPLACE VIEW top_variants AS
SELECT 
    v.id,
    v.variant_name,
    v.parent_bot_id,
    b.name as parent_bot_name,
    v.generation,
    v.trades_count,
    v.win_rate,
    v.profit_factor,
    v.total_pnl,
    v.created_at
FROM bot_variants v
LEFT JOIN bots b ON b.id = v.parent_bot_id
WHERE v.status = 'testing' AND v.trades_count >= 20
ORDER BY v.profit_factor DESC, v.win_rate DESC
LIMIT 20;
