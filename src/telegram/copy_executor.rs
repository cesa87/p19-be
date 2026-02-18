use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use chrono::Utc;

use crate::broker::oanda::OandaClient;
use crate::telegram::signal_parser::TradeDirection;
use super::signal_listener::{LiveSignal, ExecutionResult, ListenerConfig};

/// Executes copy trades from signals
pub struct CopyExecutor {
    broker: Arc<OandaClient>,
    config: Arc<RwLock<ListenerConfig>>,
    daily_trade_count: Arc<RwLock<u32>>,
    last_reset_date: Arc<RwLock<String>>,
}

impl CopyExecutor {
    pub fn new(broker: Arc<OandaClient>, config: Arc<RwLock<ListenerConfig>>) -> Self {
        Self {
            broker,
            config,
            daily_trade_count: Arc::new(RwLock::new(0)),
            last_reset_date: Arc::new(RwLock::new(String::new())),
        }
    }

    /// Check and reset daily trade count if new day
    async fn check_daily_reset(&self) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut last_date = self.last_reset_date.write().await;
        
        if *last_date != today {
            *self.daily_trade_count.write().await = 0;
            *last_date = today;
            info!("Daily trade count reset");
        }
    }

    /// Get remaining trades for today
    pub async fn remaining_trades(&self) -> u32 {
        self.check_daily_reset().await;
        let config = self.config.read().await;
        let count = *self.daily_trade_count.read().await;
        config.max_daily_trades.saturating_sub(count)
    }

    /// Execute a signal as a trade
    pub async fn execute(&self, signal: &LiveSignal) -> ExecutionResult {
        self.check_daily_reset().await;

        let config = self.config.read().await;
        
        // Check daily limit
        let current_count = *self.daily_trade_count.read().await;
        if current_count >= config.max_daily_trades {
            return ExecutionResult {
                success: false,
                order_id: None,
                fill_price: None,
                error: Some(format!("Daily trade limit reached ({}/{})", 
                    current_count, config.max_daily_trades)),
                executed_at: Utc::now(),
            };
        }

        // Check confidence
        if signal.signal.confidence < config.min_confidence {
            return ExecutionResult {
                success: false,
                order_id: None,
                fill_price: None,
                error: Some(format!("Signal confidence too low: {:.0}% (min: {:.0}%)", 
                    signal.signal.confidence * 100.0, 
                    config.min_confidence * 100.0)),
                executed_at: Utc::now(),
            };
        }

        let direction = match signal.signal.direction {
            TradeDirection::Buy => "BUY",
            TradeDirection::Sell => "SELL",
        };

        let instrument = &signal.signal.symbol;
        let lot_size = config.lot_size;
        let stop_loss = signal.signal.stop_loss;
        let take_profit = signal.signal.take_profit.first().copied();

        info!("📈 Executing copy trade: {} {} @ lot_size={}, SL={:?}, TP={:?}",
            direction, instrument, lot_size, stop_loss, take_profit);

        // Place the order
        match self.broker.place_order(
            instrument,
            direction,
            lot_size,
            stop_loss,
            take_profit,
        ).await {
            Ok(order) => {
                // Increment daily count
                *self.daily_trade_count.write().await += 1;

                info!("✅ Order placed successfully: ID={}, Price={}", order.id, order.price);

                ExecutionResult {
                    success: true,
                    order_id: Some(order.id),
                    fill_price: Some(order.price),
                    error: None,
                    executed_at: Utc::now(),
                }
            }
            Err(e) => {
                error!("❌ Order failed: {}", e);
                
                ExecutionResult {
                    success: false,
                    order_id: None,
                    fill_price: None,
                    error: Some(format!("Order failed: {}", e)),
                    executed_at: Utc::now(),
                }
            }
        }
    }

    /// Execute with custom lot size (for manual execution)
    pub async fn execute_with_lot_size(&self, signal: &LiveSignal, lot_size: f64) -> ExecutionResult {
        self.check_daily_reset().await;

        let config = self.config.read().await;
        
        // Check daily limit
        let current_count = *self.daily_trade_count.read().await;
        if current_count >= config.max_daily_trades {
            return ExecutionResult {
                success: false,
                order_id: None,
                fill_price: None,
                error: Some(format!("Daily trade limit reached ({}/{})", 
                    current_count, config.max_daily_trades)),
                executed_at: Utc::now(),
            };
        }

        let direction = match signal.signal.direction {
            TradeDirection::Buy => "BUY",
            TradeDirection::Sell => "SELL",
        };

        let instrument = &signal.signal.symbol;
        let stop_loss = signal.signal.stop_loss;
        let take_profit = signal.signal.take_profit.first().copied();

        info!("📈 Executing manual copy trade: {} {} @ lot_size={}, SL={:?}, TP={:?}",
            direction, instrument, lot_size, stop_loss, take_profit);

        match self.broker.place_order(
            instrument,
            direction,
            lot_size,
            stop_loss,
            take_profit,
        ).await {
            Ok(order) => {
                *self.daily_trade_count.write().await += 1;

                info!("✅ Order placed successfully: ID={}, Price={}", order.id, order.price);

                ExecutionResult {
                    success: true,
                    order_id: Some(order.id),
                    fill_price: Some(order.price),
                    error: None,
                    executed_at: Utc::now(),
                }
            }
            Err(e) => {
                error!("❌ Order failed: {}", e);
                
                ExecutionResult {
                    success: false,
                    order_id: None,
                    fill_price: None,
                    error: Some(format!("Order failed: {}", e)),
                    executed_at: Utc::now(),
                }
            }
        }
    }
}
