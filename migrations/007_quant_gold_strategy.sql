-- Add Quantitative Gold Momentum Reversion Strategy
-- A statistical strategy designed specifically for XAUUSD

INSERT INTO strategies (
    id,
    user_id,
    name,
    strategy_type,
    params,
    risk,
    is_active
) VALUES (
    'b0000000-0000-0000-0000-000000000012',
    'a0000000-0000-0000-0000-000000000001',
    'Quant Gold Momentum',
    'quant_gold_momentum',
    '{
        "symbol": "XAU_USD",
        "timeframe": "M15",
        "description": "Statistical momentum/mean-reversion hybrid for XAUUSD. Uses 200 EMA trend filter, RSI pullback entries, ATR-based value zone filter, and momentum confirmation.",
        "rsi_oversold": 35,
        "rsi_overbought": 65,
        "atr_distance": 1.5,
        "sl_atr_mult": 1.5,
        "tp_atr_mult": 2.0,
        "trend_ema": 200,
        "value_ema": 21,
        "rsi_period": 14
    }',
    '{
        "sl_pips": 15,
        "tp_pips": 20,
        "risk_per_trade": 1.0
    }',
    true
);
