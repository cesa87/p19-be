-- Strategy Profiles: Define optimal conditions for each strategy type
-- This enables the MRATE Orchestrator to recommend which bots should be active

-- Add profile columns to strategies table
ALTER TABLE strategies ADD COLUMN IF NOT EXISTS profile JSONB;

-- Profile schema:
-- {
--   "preferred_regimes": ["TREND", "CHOPPY"],  -- MRATE regimes where this works best
--   "best_sessions": ["LONDON", "NEW_YORK"],   -- Trading sessions
--   "hours_utc": {"start": 8, "end": 20},      -- Best hours (UTC)
--   "volatility_pref": "low",                  -- low, normal, high, any
--   "min_adx": 20,                             -- Minimum ADX for entry
--   "rsi_range": {"min": 30, "max": 70},       -- Preferred RSI range
--   "min_mrate_weight": 0.4,                   -- Minimum MRATE category weight
--   "adaptive": false                          -- If true, follows MRATE regime
-- }

-- Update existing strategies with profiles based on their type

-- RSI Reversion strategies - work best in choppy, low volatility
UPDATE strategies SET profile = '{
  "preferred_regimes": ["CHOPPY"],
  "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
  "hours_utc": {"start": 7, "end": 21},
  "volatility_pref": "low",
  "min_adx": 0,
  "rsi_range": {"min": 20, "max": 80},
  "min_mrate_weight": 0.4,
  "adaptive": false
}'::jsonb
WHERE strategy_type IN ('rsi_reversal', 'rsi_reversion_2');

-- Bollinger Bounce - mean reversion in ranging markets
UPDATE strategies SET profile = '{
  "preferred_regimes": ["CHOPPY"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 8, "end": 20},
  "volatility_pref": "normal",
  "min_adx": 0,
  "rsi_range": {"min": 25, "max": 75},
  "min_mrate_weight": 0.4,
  "adaptive": false
}'::jsonb
WHERE strategy_type = 'bollinger_bounce';

-- London/NY Trend strategies - need trending markets during sessions
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "GOLD_SUPER_BULL", "BTC_SUPER_BULL"],
  "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
  "hours_utc": {"start": 7, "end": 21},
  "volatility_pref": "normal",
  "min_adx": 20,
  "rsi_range": {"min": 30, "max": 70},
  "min_mrate_weight": 0.5,
  "adaptive": false
}'::jsonb
WHERE strategy_type IN ('london_ny_trend_continuation', 'london_ny_trend_2');

-- Breakout strategies - need volatility expansion
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "BTC_SUPER_BULL"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 7, "end": 20},
  "volatility_pref": "high",
  "min_adx": 20,
  "rsi_range": {"min": 35, "max": 65},
  "min_mrate_weight": 0.5,
  "adaptive": false
}'::jsonb
WHERE strategy_type IN ('atr_breakout', 'donchian_breakout', 'london_breakout', 'volatility_expansion');

-- MRATE Regime Trader - adaptive, follows MRATE
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "GOLD_SUPER_BULL", "BTC_SUPER_BULL"],
  "best_sessions": ["LONDON", "NEW_YORK", "OVERLAP"],
  "hours_utc": {"start": 0, "end": 24},
  "volatility_pref": "any",
  "min_adx": 20,
  "rsi_range": {"min": 0, "max": 100},
  "min_mrate_weight": 0.0,
  "adaptive": true
}'::jsonb
WHERE strategy_type = 'mrate_regime_trader';

-- Macro Aligned Momentum - needs macro alignment
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "GOLD_SUPER_BULL"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 8, "end": 20},
  "volatility_pref": "normal",
  "min_adx": 20,
  "rsi_range": {"min": 40, "max": 60},
  "min_mrate_weight": 0.6,
  "adaptive": false
}'::jsonb
WHERE strategy_type = 'macro_aligned_momentum';

-- Liquidity Sweep - works in various conditions
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "CHOPPY"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 7, "end": 20},
  "volatility_pref": "normal",
  "min_adx": 15,
  "rsi_range": {"min": 30, "max": 70},
  "min_mrate_weight": 0.4,
  "adaptive": false
}'::jsonb
WHERE strategy_type = 'liquidity_sweep';

-- MACD strategies - trend following
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 8, "end": 20},
  "volatility_pref": "normal",
  "min_adx": 20,
  "rsi_range": {"min": 35, "max": 65},
  "min_mrate_weight": 0.5,
  "adaptive": false
}'::jsonb
WHERE strategy_type IN ('macd_crossover', 'ma_crossover', 'ema_crossover');

-- Asian Range - specific to Asian session
UPDATE strategies SET profile = '{
  "preferred_regimes": ["CHOPPY", "TREND"],
  "best_sessions": ["ASIAN"],
  "hours_utc": {"start": 0, "end": 8},
  "volatility_pref": "low",
  "min_adx": 0,
  "rsi_range": {"min": 30, "max": 70},
  "min_mrate_weight": 0.3,
  "adaptive": false
}'::jsonb
WHERE strategy_type = 'asian_range';

-- Panic regime strategies - safe haven
UPDATE strategies SET profile = '{
  "preferred_regimes": ["PANIC", "GOLD_SUPER_BULL"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 0, "end": 24},
  "volatility_pref": "high",
  "min_adx": 0,
  "rsi_range": {"min": 0, "max": 100},
  "min_mrate_weight": 0.0,
  "adaptive": false
}'::jsonb
WHERE strategy_type = 'stochastic_reversal';

-- Default profile for any strategies without one
UPDATE strategies SET profile = '{
  "preferred_regimes": ["TREND", "CHOPPY"],
  "best_sessions": ["LONDON", "NEW_YORK"],
  "hours_utc": {"start": 8, "end": 20},
  "volatility_pref": "normal",
  "min_adx": 15,
  "rsi_range": {"min": 30, "max": 70},
  "min_mrate_weight": 0.4,
  "adaptive": false
}'::jsonb
WHERE profile IS NULL;

-- Market coverage gaps table for gap detection
CREATE TABLE IF NOT EXISTS market_coverage_gaps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    regime VARCHAR(30) NOT NULL,
    session VARCHAR(20) NOT NULL,
    hour_utc INTEGER NOT NULL,
    volatility_level VARCHAR(20),
    best_strategy_score DECIMAL(5,2),
    occurrence_count INTEGER DEFAULT 1,
    first_seen TIMESTAMPTZ DEFAULT NOW(),
    last_seen TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(regime, session, hour_utc)
);

CREATE INDEX IF NOT EXISTS idx_coverage_gaps_frequency ON market_coverage_gaps(occurrence_count DESC);
