use serde::{Deserialize, Serialize};

/// Supported technical indicators
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Indicator {
    Sma { period: usize },
    Ema { period: usize },
    Rsi { period: usize },
    BollingerBands { period: usize, std_dev: f64 },
    Atr { period: usize },
}

/// Comparison operators for conditions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Comparison {
    GreaterThan,
    LessThan,
    CrossesAbove,
    CrossesBelow,
}

/// What value to compare against
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompareValue {
    /// Compare to a fixed number (e.g., RSI > 70)
    Fixed { value: f64 },
    /// Compare to the current price
    Price,
    /// Compare to another indicator
    Indicator { indicator: Box<Indicator> },
}

/// A single condition for entry or exit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub indicator: Indicator,
    pub comparison: Comparison,
    pub compare_to: CompareValue,
}

/// How to combine multiple conditions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LogicOperator {
    And,
    Or,
}

/// Entry/exit rule with one or more conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub conditions: Vec<Condition>,
    pub logic: LogicOperator,
}

/// Position sizing method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionSizing {
    /// Fixed lot size
    FixedLots { lots: f64 },
    /// Percentage of account balance
    PercentOfBalance { percent: f64 },
    /// Risk a percentage of balance per trade (requires stop loss)
    RiskPercent { percent: f64 },
}

/// Complete strategy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDefinition {
    pub name: String,
    pub symbol: String,
    pub timeframe: String,
    
    /// Conditions to enter a LONG position
    pub entry_long: Rule,
    /// Conditions to exit a LONG position (optional - can use stop/take profit)
    pub exit_long: Option<Rule>,
    /// Conditions to enter a SHORT position
    pub entry_short: Option<Rule>,
    /// Conditions to exit a SHORT position
    pub exit_short: Option<Rule>,
    
    /// Stop loss in pips (optional)
    pub stop_loss_pips: Option<f64>,
    /// Take profit in pips (optional)
    pub take_profit_pips: Option<f64>,
    /// Trailing stop in pips (optional)
    pub trailing_stop_pips: Option<f64>,
    
    /// Position sizing
    pub position_sizing: PositionSizing,
}

/// Template strategies that users can start with
pub mod templates {
    use super::*;
    
    /// Simple SMA crossover strategy
    /// Long when fast SMA crosses above slow SMA
    /// Short when fast SMA crosses below slow SMA
    pub fn sma_crossover(fast_period: usize, slow_period: usize) -> StrategyDefinition {
        StrategyDefinition {
            name: format!("SMA Crossover ({}/{})", fast_period, slow_period),
            symbol: "XAUUSD".to_string(),
            timeframe: "H4".to_string(),
            entry_long: Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Sma { period: fast_period },
                    comparison: Comparison::CrossesAbove,
                    compare_to: CompareValue::Indicator {
                        indicator: Box::new(Indicator::Sma { period: slow_period }),
                    },
                }],
                logic: LogicOperator::And,
            },
            exit_long: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Sma { period: fast_period },
                    comparison: Comparison::CrossesBelow,
                    compare_to: CompareValue::Indicator {
                        indicator: Box::new(Indicator::Sma { period: slow_period }),
                    },
                }],
                logic: LogicOperator::And,
            }),
            entry_short: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Sma { period: fast_period },
                    comparison: Comparison::CrossesBelow,
                    compare_to: CompareValue::Indicator {
                        indicator: Box::new(Indicator::Sma { period: slow_period }),
                    },
                }],
                logic: LogicOperator::And,
            }),
            exit_short: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Sma { period: fast_period },
                    comparison: Comparison::CrossesAbove,
                    compare_to: CompareValue::Indicator {
                        indicator: Box::new(Indicator::Sma { period: slow_period }),
                    },
                }],
                logic: LogicOperator::And,
            }),
            stop_loss_pips: Some(50.0),
            take_profit_pips: Some(100.0),
            trailing_stop_pips: None,
            position_sizing: PositionSizing::FixedLots { lots: 0.1 },
        }
    }
    
    /// RSI overbought/oversold strategy
    /// Long when RSI crosses below oversold level (e.g., 30)
    /// Short when RSI crosses above overbought level (e.g., 70)
    pub fn rsi_reversal(period: usize, oversold: f64, overbought: f64) -> StrategyDefinition {
        StrategyDefinition {
            name: format!("RSI Reversal ({}, {}/{})", period, oversold, overbought),
            symbol: "XAUUSD".to_string(),
            timeframe: "H4".to_string(),
            entry_long: Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Rsi { period },
                    comparison: Comparison::CrossesAbove,
                    compare_to: CompareValue::Fixed { value: oversold },
                }],
                logic: LogicOperator::And,
            },
            exit_long: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Rsi { period },
                    comparison: Comparison::GreaterThan,
                    compare_to: CompareValue::Fixed { value: overbought },
                }],
                logic: LogicOperator::And,
            }),
            entry_short: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Rsi { period },
                    comparison: Comparison::CrossesBelow,
                    compare_to: CompareValue::Fixed { value: overbought },
                }],
                logic: LogicOperator::And,
            }),
            exit_short: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Rsi { period },
                    comparison: Comparison::LessThan,
                    compare_to: CompareValue::Fixed { value: oversold },
                }],
                logic: LogicOperator::And,
            }),
            stop_loss_pips: Some(30.0),
            take_profit_pips: Some(60.0),
            trailing_stop_pips: None,
            position_sizing: PositionSizing::FixedLots { lots: 0.1 },
        }
    }
    
    /// Bollinger Band bounce strategy
    /// Long when price crosses above lower band
    /// Short when price crosses below upper band
    pub fn bollinger_bounce(period: usize, std_dev: f64) -> StrategyDefinition {
        StrategyDefinition {
            name: format!("Bollinger Bounce ({}, {}σ)", period, std_dev),
            symbol: "XAUUSD".to_string(),
            timeframe: "H4".to_string(),
            entry_long: Rule {
                conditions: vec![Condition {
                    indicator: Indicator::BollingerBands { period, std_dev },
                    comparison: Comparison::CrossesAbove,
                    compare_to: CompareValue::Price,
                }],
                logic: LogicOperator::And,
            },
            exit_long: Some(Rule {
                conditions: vec![Condition {
                    indicator: Indicator::Sma { period }, // Exit at middle band
                    comparison: Comparison::LessThan,
                    compare_to: CompareValue::Price,
                }],
                logic: LogicOperator::And,
            }),
            entry_short: None, // Long-only for simplicity
            exit_short: None,
            stop_loss_pips: Some(40.0),
            take_profit_pips: Some(80.0),
            trailing_stop_pips: None,
            position_sizing: PositionSizing::FixedLots { lots: 0.1 },
        }
    }
}
