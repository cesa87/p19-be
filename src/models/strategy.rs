use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    Custom,
    MaCrossover,
    EmaCrossover,
    RsiReversal,
    #[serde(rename = "rsi_reversion_2")]
    RsiReversion2,
    MacdCrossover,
    StochasticReversal,
    BollingerBounce,
    AtrBreakout,
    DonchianBreakout,
    LondonBreakout,
    AsianRange,
    SupportResistance,
    QuantGoldMomentum,
    LondonNyTrendContinuation,
    #[serde(rename = "london_ny_trend_2")]
    LondonNyTrend2,
    ForecastConfidence,
    VolatilityExpansion,
    TrendlineBounce,
    LiquiditySweep,
    MacroAlignedMomentum,
    MrateRegimeTrader,
    RealYieldMomentum,
    GoldDxyDivergence,
    FomcVolatility,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyType::Custom => write!(f, "custom"),
            StrategyType::MaCrossover => write!(f, "ma_crossover"),
            StrategyType::EmaCrossover => write!(f, "ema_crossover"),
            StrategyType::RsiReversal => write!(f, "rsi_reversal"),
            StrategyType::RsiReversion2 => write!(f, "rsi_reversion_2"),
            StrategyType::MacdCrossover => write!(f, "macd_crossover"),
            StrategyType::StochasticReversal => write!(f, "stochastic_reversal"),
            StrategyType::BollingerBounce => write!(f, "bollinger_bounce"),
            StrategyType::AtrBreakout => write!(f, "atr_breakout"),
            StrategyType::DonchianBreakout => write!(f, "donchian_breakout"),
            StrategyType::LondonBreakout => write!(f, "london_breakout"),
            StrategyType::AsianRange => write!(f, "asian_range"),
            StrategyType::SupportResistance => write!(f, "support_resistance"),
            StrategyType::QuantGoldMomentum => write!(f, "quant_gold_momentum"),
            StrategyType::LondonNyTrendContinuation => write!(f, "london_ny_trend_continuation"),
            StrategyType::LondonNyTrend2 => write!(f, "london_ny_trend_2"),
            StrategyType::ForecastConfidence => write!(f, "forecast_confidence"),
            StrategyType::VolatilityExpansion => write!(f, "volatility_expansion"),
            StrategyType::TrendlineBounce => write!(f, "trendline_bounce"),
            StrategyType::LiquiditySweep => write!(f, "liquidity_sweep"),
            StrategyType::MacroAlignedMomentum => write!(f, "macro_aligned_momentum"),
            StrategyType::MrateRegimeTrader => write!(f, "mrate_regime_trader"),
            StrategyType::RealYieldMomentum => write!(f, "real_yield_momentum"),
            StrategyType::GoldDxyDivergence => write!(f, "gold_dxy_divergence"),
            StrategyType::FomcVolatility => write!(f, "fomc_volatility"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskParams {
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
    pub position_size_pct: f64,
    pub max_daily_loss_pct: f64,
}

impl Default for RiskParams {
    fn default() -> Self {
        Self {
            stop_loss_pct: 1.0,
            take_profit_pct: 2.0,
            position_size_pct: 2.0,
            max_daily_loss_pct: 5.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Strategy {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub strategy_type: String,
    pub params: serde_json::Value,
    pub risk: serde_json::Value,
    pub custom_rules: Option<serde_json::Value>,
    pub is_active: bool,
    /// MRATE category: trend, breakout, mean_reversion, liquidity_sweep, or None
    pub mrate_category: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStrategyRequest {
    pub name: String,
    pub strategy_type: StrategyType,
    pub params: serde_json::Value,
    pub risk: RiskParams,
    pub custom_rules: Option<serde_json::Value>,
    pub mrate_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStrategyRequest {
    pub name: Option<String>,
    pub strategy_type: Option<StrategyType>,
    pub params: Option<serde_json::Value>,
    pub risk: Option<RiskParams>,
    pub custom_rules: Option<serde_json::Value>,
    pub is_active: Option<bool>,
    pub mrate_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyResponse {
    pub id: Uuid,
    pub name: String,
    pub strategy_type: String,
    pub params: serde_json::Value,
    pub risk: RiskParams,
    pub custom_rules: Option<serde_json::Value>,
    pub is_active: bool,
    pub mrate_category: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Strategy> for StrategyResponse {
    fn from(s: Strategy) -> Self {
        let risk: RiskParams = serde_json::from_value(s.risk).unwrap_or_default();
        Self {
            id: s.id,
            name: s.name,
            strategy_type: s.strategy_type,
            params: s.params,
            risk,
            custom_rules: s.custom_rules,
            is_active: s.is_active,
            mrate_category: s.mrate_category,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
