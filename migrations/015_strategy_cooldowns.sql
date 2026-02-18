-- Add cooldown_seconds to strategy params
-- Mean reversion strategies: shorter cooldown (120s) to catch bounces
-- Trend/breakout strategies: longer cooldown (300-600s) to avoid chasing

-- RSI Mean Reversion - 2 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 120}'::jsonb
WHERE strategy_type = 'rsi_mean_reversion'
AND NOT (params ? 'cooldown_seconds');

-- Bollinger Mean Reversion - 2 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 120}'::jsonb
WHERE strategy_type = 'bollinger_mean_reversion'
AND NOT (params ? 'cooldown_seconds');

-- Stochastic Crossover - 2.5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 150}'::jsonb
WHERE strategy_type = 'stochastic_crossover'
AND NOT (params ? 'cooldown_seconds');

-- MACD Crossover - 5 min cooldown (trend following)
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'macd_crossover'
AND NOT (params ? 'cooldown_seconds');

-- MACD Divergence - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'macd_divergence'
AND NOT (params ? 'cooldown_seconds');

-- Donchian Breakout - 10 min cooldown (avoid false breakouts)
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 600}'::jsonb
WHERE strategy_type = 'donchian_breakout'
AND NOT (params ? 'cooldown_seconds');

-- ADX Trend - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'adx_trend'
AND NOT (params ? 'cooldown_seconds');

-- London Breakout - 10 min cooldown (one shot per session)
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 600}'::jsonb
WHERE strategy_type = 'london_breakout'
AND NOT (params ? 'cooldown_seconds');

-- EMA Ribbon - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'ema_ribbon'
AND NOT (params ? 'cooldown_seconds');

-- Triple Screen - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'triple_screen'
AND NOT (params ? 'cooldown_seconds');

-- Quant Gold Momentum - 3 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 180}'::jsonb
WHERE strategy_type = 'quant_gold_momentum'
AND NOT (params ? 'cooldown_seconds');

-- London NY Trend Continuation - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'london_ny_trend_continuation'
AND NOT (params ? 'cooldown_seconds');

-- Forecast Confidence - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'forecast_confidence'
AND NOT (params ? 'cooldown_seconds');

-- Volatility Expansion - 10 min cooldown (rare setups)
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 600}'::jsonb
WHERE strategy_type = 'volatility_expansion'
AND NOT (params ? 'cooldown_seconds');

-- Trendline Bounce - 3 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 180}'::jsonb
WHERE strategy_type = 'trendline_bounce'
AND NOT (params ? 'cooldown_seconds');

-- Liquidity Sweep - 2 min cooldown (quick reversals)
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 120}'::jsonb
WHERE strategy_type = 'liquidity_sweep'
AND NOT (params ? 'cooldown_seconds');

-- Macro Aligned Momentum - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'macro_aligned_momentum'
AND NOT (params ? 'cooldown_seconds');

-- MRATE Regime Trader - 5 min cooldown
UPDATE strategies 
SET params = params || '{"cooldown_seconds": 300}'::jsonb
WHERE strategy_type = 'mrate_regime_trader'
AND NOT (params ? 'cooldown_seconds');
