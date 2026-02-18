-- Backfill strategy profiles with corrected regime/session coverage
-- Ensures PANIC regime and ASIAN session have adequate bot coverage

-- Gold momentum strategies: add PANIC (safe haven), ASIAN session
UPDATE strategies SET profile = jsonb_set(
    jsonb_set(
        COALESCE(profile, '{}'::jsonb),
        '{preferred_regimes}',
        '["TREND", "GOLD_SUPER_BULL", "PANIC"]'::jsonb
    ),
    '{best_sessions}',
    '["LONDON", "NEW_YORK", "OVERLAP", "ASIAN"]'::jsonb
)
WHERE strategy_type = 'quant_gold_momentum';

-- Real yield momentum: add PANIC (gold safe haven driven by yields)
UPDATE strategies SET profile = jsonb_set(
    COALESCE(profile, '{}'::jsonb),
    '{preferred_regimes}',
    '["TREND", "GOLD_SUPER_BULL", "PANIC"]'::jsonb
)
WHERE strategy_type = 'real_yield_momentum';

-- RSI mean reversion strategies: add PANIC (oversold bounces), ASIAN session
UPDATE strategies SET profile = jsonb_set(
    jsonb_set(
        COALESCE(profile, '{}'::jsonb),
        '{preferred_regimes}',
        '["CHOPPY", "TREND", "PANIC"]'::jsonb
    ),
    '{best_sessions}',
    '["LONDON", "NEW_YORK", "ASIAN"]'::jsonb
)
WHERE strategy_type IN ('rsi_reversion_2', 'rsi_mean_reversion', 'rsi_reversal');

-- Bollinger strategies: add PANIC, ASIAN session
UPDATE strategies SET profile = jsonb_set(
    jsonb_set(
        COALESCE(profile, '{}'::jsonb),
        '{preferred_regimes}',
        '["CHOPPY", "PANIC"]'::jsonb
    ),
    '{best_sessions}',
    '["LONDON", "NEW_YORK", "ASIAN"]'::jsonb
)
WHERE strategy_type IN ('bollinger_mean_reversion', 'bollinger_bounce');

-- Stochastic strategies: add PANIC
UPDATE strategies SET profile = jsonb_set(
    COALESCE(profile, '{}'::jsonb),
    '{preferred_regimes}',
    '["CHOPPY", "PANIC"]'::jsonb
)
WHERE strategy_type IN ('stochastic_crossover', 'stochastic_reversal');

-- Liquidity sweep: add TREND and LONDON
UPDATE strategies SET profile = jsonb_set(
    jsonb_set(
        COALESCE(profile, '{}'::jsonb),
        '{preferred_regimes}',
        '["CHOPPY", "PANIC", "TREND"]'::jsonb
    ),
    '{best_sessions}',
    '["NEW_YORK", "ASIAN", "LONDON"]'::jsonb
)
WHERE strategy_type = 'liquidity_sweep';

-- MRATE regime trader: all regimes, all sessions, 24h
UPDATE strategies SET profile = jsonb_set(
    jsonb_set(
        jsonb_set(
            COALESCE(profile, '{}'::jsonb),
            '{preferred_regimes}',
            '["TREND", "CHOPPY", "GOLD_SUPER_BULL", "BTC_SUPER_BULL", "PANIC"]'::jsonb
        ),
        '{best_sessions}',
        '["LONDON", "NEW_YORK", "OVERLAP", "ASIAN"]'::jsonb
    ),
    '{hours_utc}',
    '{"start": 0, "end": 24}'::jsonb
)
WHERE strategy_type = 'mrate_regime_trader';

-- Macro aligned momentum: add PANIC for gold variant
UPDATE strategies SET profile = jsonb_set(
    COALESCE(profile, '{}'::jsonb),
    '{preferred_regimes}',
    '["GOLD_SUPER_BULL", "BTC_SUPER_BULL", "TREND", "PANIC"]'::jsonb
)
WHERE strategy_type = 'macro_aligned_momentum';

-- Backfill any strategies that still have NULL profiles with a sensible default
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
