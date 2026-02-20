//! Bot Runner - The main trading loop
//! 
//! Monitors the market, generates signals, and places trades

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::broker::{OandaClient, oanda::units_per_lot};
use crate::config::Config;
use crate::models::{Bot, Strategy, Candle};
use crate::models::trade_context::{TradeContextBuilder, insert_trade_context};
use crate::bot::signals::{generate_signal, Signal};
use crate::bot::activity::{log_activity, ActivityType};
use crate::engine::indicators::{rsi, adx, atr, ema};
use crate::macro_sentiment::{PolymarketClient, MacroSentiment};
use crate::mrate::{StrategyCategory, MrateOutput, MrateState, default_mrate_output, get_current_mrate};
use crate::risk::{PortfolioRiskEngine, RiskDecision, OpenPosition};
use crate::analytics::learning::calculate_learning_multiplier;
use chrono::Timelike;

/// Tracks running bot tasks
pub struct BotManager {
    running_bots: Arc<RwLock<HashMap<Uuid, tokio::task::JoinHandle<()>>>>,
    pool: sqlx::PgPool,
    config: Config,
    mrate_state: MrateState,
    risk_engine: Arc<PortfolioRiskEngine>,
}

impl BotManager {
    pub async fn new(pool: sqlx::PgPool, config: Config, mrate_state: MrateState) -> Self {
        // Fetch actual equity from OANDA account
        let starting_equity = Self::fetch_account_equity(&config).await;
        let risk_engine = Arc::new(PortfolioRiskEngine::new(starting_equity));
        
        tracing::info!("💰 Portfolio Risk Engine initialized with ${:.2} equity (from OANDA)", starting_equity);
        
        Self {
            running_bots: Arc::new(RwLock::new(HashMap::new())),
            pool,
            config,
            mrate_state,
            risk_engine,
        }
    }
    
    /// Fetch account equity from OANDA, fallback to $100,000 if unavailable
    async fn fetch_account_equity(config: &Config) -> f64 {
        let api_token = match &config.oanda_api_token {
            Some(t) => t.clone(),
            None => {
                tracing::warn!("No OANDA API token configured, using $100,000 default equity");
                return 100_000.0;
            }
        };
        let account_id = match &config.oanda_account_id {
            Some(a) => a.clone(),
            None => {
                tracing::warn!("No OANDA account ID configured, using $100,000 default equity");
                return 100_000.0;
            }
        };
        
        match OandaClient::new(&api_token, &account_id, config.oanda_practice) {
            Ok(client) => {
                match client.get_account_summary().await {
                    Ok(summary) => {
                        let nav = summary.nav.parse::<f64>().unwrap_or(100_000.0);
                        tracing::info!("📊 OANDA account: balance={}, NAV={}, currency={}", 
                            summary.balance, summary.nav, summary.currency);
                        nav
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch OANDA account summary: {}, using $100,000 default", e);
                        100_000.0
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create OANDA client: {}, using $100,000 default", e);
                100_000.0
            }
        }
    }
    
    /// Start a bot's trading loop
    pub async fn start_bot(&self, bot_id: Uuid) -> Result<(), String> {
        // Check if already running
        {
            let running = self.running_bots.read().await;
            if running.contains_key(&bot_id) {
                return Err("Bot is already running".to_string());
            }
        }
        
        // Load bot and strategy from DB
        let bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = $1")
            .bind(bot_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Bot not found")?;
        
        let strategy_id = bot.strategy_id.ok_or("Bot has no strategy")?;
        
        let strategy = sqlx::query_as::<_, Strategy>("SELECT * FROM strategies WHERE id = $1")
            .bind(strategy_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Strategy not found")?;
        
        // Determine credentials (bot-specific or global)
        let api_token = bot.api_token.clone()
            .or_else(|| self.config.oanda_api_token.clone())
            .ok_or("No API token configured")?;
        let account_id = bot.account_id.clone()
            .or_else(|| self.config.oanda_account_id.clone())
            .ok_or("No account ID configured")?;
        let is_practice = self.config.oanda_practice;
        
        // Get timeframe from strategy params (default to M15)
        let timeframe = strategy.params.get("timeframe")
            .and_then(|v| v.as_str())
            .unwrap_or("M15")
            .to_string();
        
        tracing::info!("Bot {} using timeframe: {}", bot.name, timeframe);
        
        // Get MRATE category from strategy
        let mrate_category = strategy.mrate_category
            .as_ref()
            .and_then(|c| StrategyCategory::from_str(c));
        
        if mrate_category.is_some() {
            tracing::info!("Bot {} has MRATE category: {:?}", bot.name, mrate_category);
        }
        
        // Create runner
        let runner = BotRunner {
            bot_id,
            strategy_id,
            bot_name: bot.name.clone(),
            strategy_type: strategy.strategy_type.clone(),
            strategy_params: strategy.params.clone(),
            timeframe,
            sl_pips: strategy.params.get("sl_pips").and_then(|v| v.as_f64()).unwrap_or(20.0),
            tp_pips: strategy.params.get("tp_pips")
                .or_else(|| strategy.params.get("tp1_pips"))
                .and_then(|v| v.as_f64())
                .unwrap_or(40.0),
            lot_size: bot.lot_size,
            max_positions: bot.max_positions,
            cooldown_seconds: bot.cooldown_seconds,
            api_token,
            account_id,
            is_practice,
            pool: self.pool.clone(),
            mrate_category,
            mrate_state: self.mrate_state.clone(),
            risk_engine: self.risk_engine.clone(),
        };
        
        // Spawn the trading loop
        let handle = tokio::spawn(async move {
            runner.run().await;
        });
        
        // Track the running bot
        {
            let mut running = self.running_bots.write().await;
            running.insert(bot_id, handle);
        }
        
        Ok(())
    }
    
    /// Stop a bot's trading loop
    pub async fn stop_bot(&self, bot_id: Uuid) -> Result<(), String> {
        let mut running = self.running_bots.write().await;
        
        if let Some(handle) = running.remove(&bot_id) {
            handle.abort();
            Ok(())
        } else {
            Err("Bot is not running".to_string())
        }
    }
    
    /// Check if a bot is running
    pub async fn is_running(&self, bot_id: Uuid) -> bool {
        let running = self.running_bots.read().await;
        running.contains_key(&bot_id)
    }
    
    /// Auto-restart bots that were marked as active in the database
    /// This should be called on backend startup to resume previously running bots
    pub async fn auto_restart_active_bots(&self) -> usize {
        let active_bots = match sqlx::query_as::<_, Bot>(
            "SELECT * FROM bots WHERE is_active = true AND strategy_id IS NOT NULL"
        )
        .fetch_all(&self.pool)
        .await {
            Ok(bots) => bots,
            Err(e) => {
                tracing::error!("Failed to fetch active bots: {}", e);
                return 0;
            }
        };
        
        if active_bots.is_empty() {
            tracing::info!("🤖 No active bots to auto-restart");
            return 0;
        }
        
        tracing::info!("🤖 Auto-restarting {} active bot(s)...", active_bots.len());
        
        let mut started_count = 0;
        for bot in active_bots {
            match self.start_bot(bot.id).await {
                Ok(()) => {
                    tracing::info!("✅ Auto-restarted bot: {} ({})", bot.name, bot.id);
                    started_count += 1;
                }
                Err(e) => {
                    tracing::warn!("⚠️ Failed to auto-restart bot {} ({}): {}", bot.name, bot.id, e);
                    // Mark the bot as inactive since we couldn't restart it
                    let _ = sqlx::query("UPDATE bots SET is_active = false WHERE id = $1")
                        .bind(bot.id)
                        .execute(&self.pool)
                        .await;
                }
            }
        }
        
        tracing::info!("🤖 Auto-restart complete: {}/{} bots started", started_count, started_count);
        started_count
    }
    
    /// Get the risk engine (for sharing with API)
    pub fn risk_engine(&self) -> Arc<PortfolioRiskEngine> {
        self.risk_engine.clone()
    }
}

/// Timeframe tier for MRATE integration gating
#[derive(Debug, Clone, Copy, PartialEq)]
enum TimeframeTier {
    /// M1, M5, M15 — no hard directional bias, local ATR stops, larger lots
    Intraday,
    /// H1, H4, D1 — D1 directional bias enforced, D1 ATR stops allowed
    Swing,
}

impl TimeframeTier {
    fn from_timeframe(tf: &str) -> Self {
        match tf {
            "M1" | "M5" | "M15" | "M30" => TimeframeTier::Intraday,
            _ => TimeframeTier::Swing, // H1, H4, D1, W1
        }
    }
    
    fn is_intraday(&self) -> bool {
        matches!(self, TimeframeTier::Intraday)
    }
}

/// The actual bot runner that executes trades
pub struct BotRunner {
    pub bot_id: Uuid,
    pub strategy_id: Uuid,  // For trade context tracking
    pub bot_name: String,
    pub strategy_type: String,
    pub strategy_params: serde_json::Value,
    pub timeframe: String,  // e.g., "M15", "H1", "H4"
    pub sl_pips: f64,
    pub tp_pips: f64,
    pub lot_size: f64,
    pub max_positions: i32,
    pub cooldown_seconds: Option<i32>,  // Bot-level override
    pub api_token: String,
    pub account_id: String,
    pub is_practice: bool,
    pub pool: sqlx::PgPool,
    pub mrate_category: Option<StrategyCategory>,
    pub mrate_state: MrateState,
    pub risk_engine: Arc<PortfolioRiskEngine>,
}

impl BotRunner {
    /// Main trading loop
    pub async fn run(&self) {
        tracing::info!("🤖 Bot {} ({}) starting...", self.bot_name, self.bot_id);
        
        // Log startup
        let _ = log_activity(
            &self.pool,
            self.bot_id,
            ActivityType::Info,
            &format!("Bot started with {} strategy", self.strategy_type),
            Some(serde_json::json!({
                "sl_pips": self.sl_pips,
                "tp_pips": self.tp_pips,
                "lot_size": self.lot_size,
            })),
        ).await;
        
        // Create OANDA client
        let client = match OandaClient::new(&self.api_token, &self.account_id, self.is_practice) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to create OANDA client: {}", e);
                let _ = log_activity(&self.pool, self.bot_id, ActivityType::Error, 
                    &format!("Failed to connect to broker: {}", e), None).await;
                return;
            }
        };
        
        // Trading loop - check every 30 seconds
        let mut ticker = interval(Duration::from_secs(30));
        let mut last_signal_time: Option<std::time::Instant> = None;
        let mut last_equity_update: Option<std::time::Instant> = None;
        
        // Cooldown priority: bot setting > strategy param > default (5 min)
        let cooldown_seconds = self.cooldown_seconds
            .map(|s| s as u64)
            .or_else(|| self.strategy_params.get("cooldown_seconds")
                .and_then(|v| v.as_u64()))
            .unwrap_or(300);
        let signal_cooldown = Duration::from_secs(cooldown_seconds);
        
        tracing::info!("Bot {} using cooldown: {}s", self.bot_name, cooldown_seconds);
        
        loop {
            ticker.tick().await;
            
            // Sync closed trades from broker (update PnL, exit reason, etc.)
            if let Err(e) = self.sync_closed_trades(&client).await {
                tracing::warn!("Failed to sync closed trades: {}", e);
            }
            
            // Reconcile DB with actual OANDA positions (catches manual closes, restarts, etc.)
            if let Err(e) = self.reconcile_positions(&client).await {
                tracing::warn!("Failed to reconcile positions: {}", e);
            }
            
            // Update account equity from broker every 5 minutes
            let should_update_equity = match last_equity_update {
                None => true,
                Some(last_time) => last_time.elapsed() > Duration::from_secs(300),
            };
            
            if should_update_equity {
                match client.get_account_summary().await {
                    Ok(account_summary) => {
                        if let Ok(nav) = account_summary.nav.parse::<f64>() {
                            self.risk_engine.update_equity(nav).await;
                            tracing::info!("💵 Account equity updated: ${:.2}", nav);
                            last_equity_update = Some(std::time::Instant::now());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch account summary: {}", e);
                    }
                }
            }
            
            // Check if bot is still active
            let is_active = sqlx::query_scalar::<_, bool>(
                "SELECT is_active FROM bots WHERE id = $1"
            )
            .bind(self.bot_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(false);
            
            if !is_active {
                tracing::info!("Bot {} stopped (is_active = false)", self.bot_id);
                let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info, 
                    "Bot stopped", None).await;
                break;
            }
            
            // Fetch recent candles for analysis
            // Need 250+ for 200 EMA to warm up properly
            let candle_count = self.strategy_params.get("candle_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(300) as u32;
            
            // Get instrument from strategy params, default to XAU_USD
            let instrument = self.strategy_params.get("instrument")
                .or_else(|| self.strategy_params.get("symbol"))
                .and_then(|v| v.as_str())
                .unwrap_or("XAU_USD");
            
            let candles = match client.get_candles(instrument, &self.timeframe, candle_count).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Failed to fetch candles: {}", e);
                    continue;
                }
            };
            
            if candles.is_empty() {
                continue;
            }
            
            // Get current price
            let current_price = candles.last().map(|c| c.close).unwrap_or(0.0);
            
            // Fetch MRATE data (used for both weight relaxation and regime-driven strategies)
            let mrate_output = self.fetch_mrate().await;
            let mrate_ref = mrate_output.as_ref();
            
            // Get MRATE weight for threshold relaxation (if category is set)
            let mrate_weight = if let Some(category) = self.mrate_category {
                mrate_ref.map(|m| m.strategy_weights.get(category))
            } else {
                None
            };
            
            // Generate signal (with MRATE data for regime-driven strategies)
            let mut signal_result = generate_signal(&self.strategy_type, &candles, &self.strategy_params, mrate_weight, mrate_ref);
            
            // Apply macro sentiment filter if enabled
            let use_macro_filter = self.strategy_params.get("use_macro_filter")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            
            let mut macro_modifier = 1.0;
            let mut macro_sentiment: Option<MacroSentiment> = None;
            
            if use_macro_filter && (signal_result.signal == Signal::Buy || signal_result.signal == Signal::Sell) {
                // Fetch macro sentiment from Polymarket
                let client = PolymarketClient::new();
                let sentiment = client.calculate_macro_sentiment().await;
                
                // Apply instrument-specific adjustments
                let instrument_modifier = match instrument {
                    "BTC_USD" => {
                        // For BTC, also factor in BTC-specific sentiment
                        let btc_factor = 1.0 + (sentiment.btc_sentiment_score * 0.1); // ±10% based on BTC sentiment
                        sentiment.confidence_modifier * btc_factor
                    },
                    "XAU_USD" => {
                        // For Gold, the base confidence_modifier already accounts for Fed/inflation
                        sentiment.confidence_modifier
                    },
                    _ => sentiment.confidence_modifier,
                };
                
                macro_modifier = instrument_modifier.clamp(0.5, 1.5);
                
                // Adjust confidence
                let original_confidence = signal_result.confidence;
                signal_result.confidence = (original_confidence * macro_modifier).clamp(0.0, 1.0);
                
                tracing::info!(
                    "Macro filter applied: original={:.2}, modifier={:.2}, final={:.2}",
                    original_confidence, macro_modifier, signal_result.confidence
                );
                
                macro_sentiment = Some(sentiment);
            }
            
            // Log the analysis with structured conditions
            let mut details = serde_json::json!({
                "price": current_price,
                "signal": format!("{:?}", signal_result.signal),
                "confidence": signal_result.confidence,
            });
            
            // Add macro sentiment info if filter was applied
            if let Some(ref sentiment) = macro_sentiment {
                details["macro_filter"] = serde_json::json!({
                    "enabled": true,
                    "modifier": macro_modifier,
                    "fed_uncertainty": sentiment.fed_uncertainty,
                    "rate_cut_probability": sentiment.fed_rate_cut_probability,
                    "btc_sentiment": sentiment.btc_sentiment_score,
                    "markets_analyzed": sentiment.markets_analyzed,
                });
            }
            
            // Add conditions if present
            if !signal_result.conditions.is_empty() {
                details["conditions"] = serde_json::to_value(&signal_result.conditions).unwrap_or_default();
            }
            if let Some(ref dir) = signal_result.direction {
                details["direction"] = serde_json::json!(dir);
            }
            
            let _ = log_activity(
                &self.pool,
                self.bot_id,
                ActivityType::Info,
                &signal_result.reason,
                Some(details),
            ).await;
            
            // Check if we should act on the signal
            let should_trade = match signal_result.signal {
                Signal::Buy | Signal::Sell => {
                    // Check cooldown
                    if let Some(last_time) = last_signal_time {
                        last_time.elapsed() > signal_cooldown
                    } else {
                        true
                    }
                }
                Signal::Hold => false,
            };
            
            if should_trade && signal_result.confidence >= 0.6 {
                // ═══════════════════════════════════════════════════════════════════
                // TIMEFRAME-AWARE MRATE FILTERING
                // Intraday (M5/M15): no hard blocking, soft lot reduction on counter-trend
                // Swing (H1+): full directional bias enforcement from D1 trend
                // ═══════════════════════════════════════════════════════════════════
                let tf_tier = TimeframeTier::from_timeframe(&self.timeframe);
                let mut mrate_lot_multiplier = 1.0;
                let mrate_output = self.fetch_mrate().await.unwrap_or_else(default_mrate_output);
                if let Some(category) = self.mrate_category {
                    let direction = match signal_result.signal {
                        Signal::Buy => Some("LONG"),
                        Signal::Sell => Some("SHORT"),
                        Signal::Hold => None,
                    };
                    
                    if tf_tier.is_intraday() {
                        // ── INTRADAY: Never block, only adjust lot size ──
                        // Get the instrument's D1 trading direction for soft filtering
                        let inst_score = mrate_output.instrument_scores.get_score(instrument);
                        let d1_direction = inst_score.map(|s| s.trading_direction);
                        
                        let direction_opposes = match (direction, d1_direction) {
                            (Some("LONG"), Some(crate::mrate::models::TradingDirection::Short)) => true,
                            (Some("SHORT"), Some(crate::mrate::models::TradingDirection::Long)) => true,
                            _ => false,
                        };
                        
                        if direction_opposes {
                            // Counter-trend intraday: reduce lots by 50%, don't block
                            mrate_lot_multiplier = 0.5 * mrate_output.risk_multiplier;
                            let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                                &format!("⚡ Intraday counter-D1: {} on {} — lot reduced 50%",
                                    direction.unwrap_or("?"), instrument),
                                Some(serde_json::json!({
                                    "tier": "intraday",
                                    "timeframe": self.timeframe,
                                    "d1_direction": format!("{:?}", d1_direction),
                                    "signal_direction": direction,
                                    "lot_multiplier": mrate_lot_multiplier,
                                }))).await;
                        } else {
                            // Aligned or neutral: use risk multiplier only (no category weight penalty)
                            mrate_lot_multiplier = mrate_output.risk_multiplier;
                        }
                        
                        tracing::info!(
                            "MRATE [INTRADAY {}]: regime={}, lot_mult={:.2}x, counter_trend={}",
                            self.timeframe, mrate_output.regime.as_str(), mrate_lot_multiplier, direction_opposes
                        );
                    } else {
                        // ── SWING (H1+): MANDATORY D1 direction check ──
                        // Step 1: Check locked direction from D1 candle vs EMA 200
                        if let Ok(Some(locked)) = crate::mrate::locked_direction::get_locked_direction(&self.pool, instrument).await {
                            let locked_dir = locked.trading_direction();
                            let signal_dir_enum = match direction {
                                Some("LONG") => crate::mrate::models::TradingDirection::Long,
                                Some("SHORT") => crate::mrate::models::TradingDirection::Short,
                                _ => crate::mrate::models::TradingDirection::Neutral,
                            };
                            
                            // HARD BLOCK if signal opposes locked D1 direction
                            let direction_opposes = match (locked_dir, signal_dir_enum) {
                                (crate::mrate::models::TradingDirection::Long, crate::mrate::models::TradingDirection::Short) => true,
                                (crate::mrate::models::TradingDirection::Short, crate::mrate::models::TradingDirection::Long) => true,
                                _ => false,
                            };
                            
                            if direction_opposes {
                                let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                                    &format!("🚫 BLOCKED [SWING {} counter-D1]: Signal {:?} opposes locked direction {:?} | D1 close: {:.2}, EMA200: {:.2}",
                                        self.timeframe, 
                                        signal_dir_enum,
                                        locked_dir,
                                        locked.d1_close_price,
                                        locked.ema_200
                                    ), 
                                    Some(serde_json::json!({
                                        "tier": "swing",
                                        "timeframe": self.timeframe,
                                        "instrument": instrument,
                                        "signal_direction": format!("{:?}", signal_dir_enum),
                                        "locked_direction": format!("{:?}", locked_dir),
                                        "d1_close_price": locked.d1_close_price,
                                        "ema_200": locked.ema_200,
                                        "locked_at": locked.locked_at.to_rfc3339(),
                                    }))).await;
                                tracing::warn!(
                                    "🚫 BLOCKED: {} {} signal opposes D1 direction {:?} (close {:.2} vs EMA {:.2})",
                                    instrument, self.timeframe, locked_dir, locked.d1_close_price, locked.ema_200
                                );
                                continue; // HARD BLOCK - skip this trade
                            }
                            
                            tracing::info!(
                                "✅ Direction aligned: {} signal {:?} matches D1 direction {:?}",
                                instrument, signal_dir_enum, locked_dir
                            );
                        } else {
                            tracing::warn!("⚠️ No locked direction found for {} - proceeding with caution", instrument);
                        }
                        
                        // Step 2: Get instrument score for lot multiplier adjustment
                        let inst_score = mrate_output.instrument_scores.get_score(instrument);
                        let mut adjusted_weight = 1.0;
                        
                        if let Some(score) = inst_score {
                            // Boost if strategy category matches instrument's best strategies
                            if score.best_strategies.contains(&category) {
                                adjusted_weight = 0.70; // Base boost for matching category
                                
                                // Additional boost based on price regime
                                match score.price_regime {
                                    crate::mrate::models::PriceRegime::Trending => {
                                        adjusted_weight += (score.trend_strength - 25.0).max(0.0) / 50.0 * 0.20; // +0-20%
                                    },
                                    crate::mrate::models::PriceRegime::Ranging => {
                                        if matches!(category, crate::mrate::models::StrategyCategory::MeanReversion | crate::mrate::models::StrategyCategory::LiquiditySweep) {
                                            adjusted_weight = 0.75; // Better for ranging
                                        }
                                    },
                                    _ => {}
                                }
                                
                                adjusted_weight = adjusted_weight.min(0.95);
                            } else {
                                // Category doesn't match - use moderate weight
                                adjusted_weight = 0.50;
                            }
                        }
                        
                        mrate_lot_multiplier = mrate_output.risk_multiplier * adjusted_weight;
                        
                        tracing::info!(
                            "MRATE [SWING {}]: regime={}, category={:?}, weight={:.0}%, lot_mult={:.2}x",
                            self.timeframe, mrate_output.regime.as_str(), category, adjusted_weight * 100.0, mrate_lot_multiplier
                        );
                        
                        if let Some(score) = inst_score {
                            if score.best_strategies.contains(&category) {
                                let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                                    &format!("✨ {} {} favors {:?} strategies — boosted to {:.0}%",
                                        instrument,
                                        format!("{:?}", score.price_regime).to_lowercase(),
                                        category,
                                        adjusted_weight * 100.0
                                    ), 
                                    Some(serde_json::json!({
                                        "instrument": instrument,
                                        "price_regime": format!("{:?}", score.price_regime),
                                        "category": category.as_str(),
                                        "adjusted_weight": adjusted_weight,
                                    }))).await;
                            }
                        }
                    }
                }
                
                // ═══════════════════════════════════════════════════════════════════
                // ADAPTIVE LEARNING - ADJUST FROM HISTORICAL PERFORMANCE
                // ═══════════════════════════════════════════════════════════════════
                let regime_str = mrate_output.regime.as_str();
                let hour = chrono::Utc::now().hour();
                let session_str = match hour {
                    0..=7 => "ASIAN",
                    8..=11 => "LONDON",
                    12..=15 => "OVERLAP",
                    16..=21 => "NEW_YORK",
                    _ => "OFF_HOURS",
                };
                let learning = calculate_learning_multiplier(
                    &self.pool, self.bot_id, regime_str, session_str
                ).await;
                
                if learning.has_sufficient_data {
                    signal_result.confidence = (signal_result.confidence * learning.confidence_mult).clamp(0.0, 1.0);
                    mrate_lot_multiplier *= learning.lot_mult;
                    tracing::info!(
                        "🧠 Learning: {} → conf={:.2}, lot_mult={:.2}x",
                        learning.summary, signal_result.confidence, mrate_lot_multiplier
                    );
                } else {
                    tracing::debug!("🧠 Learning: {}", learning.summary);
                }
                
                // Re-check confidence after learning adjustment — may have dropped below threshold
                if signal_result.confidence < 0.6 {
                    let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                        &format!("🧠 Learning dropped confidence to {:.2} (below 0.6 threshold) — skipping trade", 
                            signal_result.confidence), 
                        Some(serde_json::json!({
                            "learning_summary": learning.summary,
                            "win_rate": learning.win_rate,
                            "profit_factor": learning.profit_factor,
                        }))).await;
                    continue;
                }
                
                // ═══════════════════════════════════════════════════════════════════
                // PORTFOLIO RISK ENGINE - THE KILL SWITCH
                // ═══════════════════════════════════════════════════════════════════
                let risk_decision = self.risk_engine.check_account_risk().await;
                
                match risk_decision {
                    RiskDecision::EmergencyShutdown => {
                        // 🚨 EMERGENCY SHUTDOWN - STOP ALL TRADING
                        tracing::error!("🚨 EMERGENCY SHUTDOWN: Risk limits breached!");
                        let _ = log_activity(
                            &self.pool,
                            self.bot_id,
                            ActivityType::Error,
                            "🚨 EMERGENCY SHUTDOWN: Risk limits breached. Stopping all trading.",
                            None,
                        ).await;
                        
                        // Stop this bot
                        let _ = sqlx::query("UPDATE bots SET is_active = false WHERE id = $1")
                            .bind(self.bot_id)
                            .execute(&self.pool)
                            .await;
                        
                        return;  // Exit trading loop immediately
                    }
                    RiskDecision::BlockNewTrades => {
                        // ⛔ Block new trades but keep running
                        let _ = log_activity(
                            &self.pool,
                            self.bot_id,
                            ActivityType::Warning,
                            "⛔ New trades blocked by risk engine (approaching limits)",
                            None,
                        ).await;
                        continue;  // Skip this trade
                    }
                    RiskDecision::ReducePositionSize(risk_multiplier) => {
                        // ⚠️ Reduce position size
                        mrate_lot_multiplier *= risk_multiplier;
                        tracing::warn!(
                            "⚠️ Risk engine reducing position size by {:.0}% (multiplier: {:.2})",
                            (1.0 - risk_multiplier) * 100.0,
                            risk_multiplier
                        );
                    }
                    RiskDecision::AllowTrading => {
                        // ✅ All clear - full throttle
                    }
                }
                
                // Check current positions for THIS BOT from trade_context
                let open_position_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM trade_context WHERE bot_id = $1 AND closed_at IS NULL"
                )
                .bind(self.bot_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
                
                if open_position_count >= self.max_positions as i64 {
                    let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                        &format!("Max positions ({}/{}) reached, skipping signal", open_position_count, self.max_positions), None).await;
                    continue;
                }
                
                // ═══════════════════════════════════════════════════════════════════
                // CROSS-BOT INSTRUMENT LIMIT
                // Prevents multiple bots from piling into the same instrument.
                // ═══════════════════════════════════════════════════════════════════
                let max_instrument_positions = self.strategy_params.get("max_instrument_positions")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(3) as i64;  // Default max 3 positions per instrument across all bots
                
                let instrument_position_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM trade_context WHERE symbol = $1 AND closed_at IS NULL"
                )
                .bind(instrument)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);
                
                if instrument_position_count >= max_instrument_positions {
                    let _ = log_activity(&self.pool, self.bot_id, ActivityType::Info,
                        &format!("⛔ Cross-bot limit: {} has {}/{} positions across all bots",
                            instrument, instrument_position_count, max_instrument_positions), None).await;
                    continue;
                }
                
                // Place the trade!
                let direction = match signal_result.signal {
                    Signal::Buy => "BUY",
                    Signal::Sell => "SELL",
                    _ => continue,
                };
                
                // Calculate adjusted lot size with MRATE + Risk multipliers
                // Intraday boost: tighter stops → proportionally larger lots
                // so risk per trade stays consistent across timeframes.
                let intraday_lot_boost = if tf_tier.is_intraday() {
                    // Get D1 ATR and local ATR to calculate proportional boost
                    let d1_atr = mrate_output.instrument_scores.get_score(instrument)
                        .map(|s| s.d1_atr)
                        .unwrap_or(0.0);
                    let local_atr_values = crate::engine::indicators::atr(&candles, 14);
                    let local_atr = local_atr_values.last().copied().unwrap_or(0.0);
                    
                    if d1_atr > 0.0 && local_atr > 0.0 {
                        // If D1 ATR is 4x local ATR, boost lots ~2x (sqrt scaling, capped)
                        let ratio = (d1_atr / local_atr).sqrt().min(2.0);
                        tracing::info!("📈 Intraday lot boost: D1_ATR={:.2}, local_ATR={:.2}, boost={:.2}x",
                            d1_atr, local_atr, ratio);
                        ratio
                    } else {
                        1.0  // No data, no boost
                    }
                } else {
                    1.0
                };
                let adjusted_lot_size = (self.lot_size * mrate_lot_multiplier * intraday_lot_boost).max(0.01);
                
                // Calculate notional exposure for this trade
                let contract_multiplier = units_per_lot(instrument);
                let proposed_notional = adjusted_lot_size * contract_multiplier * current_price;
                
                // Check exposure limits
                let exposure_decision = self.risk_engine.check_exposure_limits(
                    proposed_notional,
                    instrument,
                    self.strategy_id,
                ).await;
                
                if exposure_decision == RiskDecision::BlockNewTrades {
                    let _ = log_activity(
                        &self.pool,
                        self.bot_id,
                        ActivityType::Warning,
                        "⛔ Trade blocked: Exposure limit would be exceeded",
                        Some(serde_json::json!({
                            "proposed_notional": proposed_notional,
                            "instrument": instrument,
                        })),
                    ).await;
                    continue;  // Skip this trade
                }
                
                // ═══════════════════════════════════════════════════════════════════
                // STRATEGY CORRELATION CHECK (Phase 3)
                // ═══════════════════════════════════════════════════════════════════
                // Check if enabled via strategy params
                let check_correlation = self.strategy_params.get("check_correlation")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);  // Default to enabled
                
                if check_correlation {
                    match self.risk_engine.check_strategy_correlation(&self.pool, self.strategy_id).await {
                        Ok(is_correlated) => {
                            if is_correlated {
                                let _ = log_activity(
                                    &self.pool,
                                    self.bot_id,
                                    ActivityType::Warning,
                                    "🔗 Trade blocked: Strategy is correlated with open position (correlation > 0.7)",
                                    Some(serde_json::json!({
                                        "strategy_id": self.strategy_id.to_string(),
                                    })),
                                ).await;
                                tracing::warn!("Strategy {} blocked due to correlation", self.strategy_id);
                                continue;  // Skip this trade
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to check strategy correlation: {}", e);
                            // Don't block trade if correlation check fails
                        }
                    }
                }
                
                // ═══════════════════════════════════════════════════════════════════
                // TIMEFRAME-AWARE STOP LOSS & TAKE PROFIT
                // ═══════════════════════════════════════════════════════════════════
                // Intraday (M5/M15): ALWAYS use local candle ATR stops — these
                //   strategies capture quick moves and need tight, timeframe-matched
                //   stops. D1 ATR would produce absurd targets (e.g., $60 oil from $66).
                // Swing (H1+): Use MRATE D1 ATR stops for room to breathe.
                // ═══════════════════════════════════════════════════════════════════
                let use_atr_stops = self.strategy_params.get("use_atr_stops")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);  // Default to ATR-based
                
                // Intraday timeframes NEVER use D1 ATR stops
                let use_d1_stops = !tf_tier.is_intraday();
                
                let (sl_price, tp_price, atr_value, stop_distance_pct, rr_ratio) = if use_atr_stops {
                    // Check if MRATE provides D1-based stop recommendations for this instrument
                    let mrate_instrument_score = mrate_output.instrument_scores.get_score(instrument);
                    let mrate_stop_available = use_d1_stops && mrate_instrument_score
                        .map(|s| s.recommended_stop_distance > 0.0)
                        .unwrap_or(false);
                    
                    if mrate_stop_available {
                        // ═══════════════════════════════════════════════════════════
                        // SWING ONLY: D1 ATR stops (gives room to breathe)
                        // ═══════════════════════════════════════════════════════════
                        let score = mrate_instrument_score.unwrap();
                        let stop_dist = score.recommended_stop_distance;
                        let tp_dist = score.recommended_tp_distance;
                        let d1_atr = score.d1_atr;
                        
                        // Apply DTE adjustments to the distances
                        let adjusted_stop = mrate_output.thresholds.adjust_stop_loss(stop_dist / d1_atr) * d1_atr;
                        let adjusted_tp = mrate_output.thresholds.adjust_take_profit(tp_dist / d1_atr) * d1_atr;
                        
                        let (sl, tp) = if direction == "BUY" {
                            (current_price - adjusted_stop, current_price + adjusted_tp)
                        } else {
                            (current_price + adjusted_stop, current_price - adjusted_tp)
                        };
                        
                        let stop_dist_pct = (adjusted_stop / current_price) * 100.0;
                        let rr = if adjusted_stop > 0.0 { adjusted_tp / adjusted_stop } else { 0.0 };
                        
                        tracing::info!(
                            "📊 D1 ATR stops [SWING {}]: D1_ATR={:.2}, mult={:.1}x, stop_dist={:.2} ({:.1}%), R:R={:.2}",
                            self.timeframe, d1_atr, score.stop_atr_multiplier, adjusted_stop, stop_dist_pct, rr
                        );
                        
                        (sl, tp, d1_atr, stop_dist_pct, rr)
                    } else {
                        // ═══════════════════════════════════════════════════════════
                        // LOCAL ATR STOPS (intraday always, swing as fallback)
                        // Uses the candle timeframe's own ATR — properly sized.
                        // ═══════════════════════════════════════════════════════════
                        let base_atr_sl_mult = self.strategy_params.get("atr_sl_multiplier")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.5);  // 1.5x ATR for stop
                        let base_atr_tp_mult = self.strategy_params.get("atr_tp_multiplier")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(3.0);  // 3.0x ATR for target (2:1 R:R)
                        
                        // Apply MRATE DTE multipliers to ATR stops/TPs
                        let atr_sl_mult = mrate_output.thresholds.adjust_stop_loss(base_atr_sl_mult);
                        let atr_tp_mult = mrate_output.thresholds.adjust_take_profit(base_atr_tp_mult);
                        
                        let trade_direction = if direction == "BUY" { "LONG" } else { "SHORT" };
                        let sl = crate::risk::calculator::calculate_atr_stop(
                            &candles,
                            atr_sl_mult,
                            trade_direction,
                        );
                        let tp = crate::risk::calculator::calculate_atr_take_profit(
                            &candles,
                            atr_tp_mult,
                            trade_direction,
                        );
                        
                        // Calculate ATR value and metrics
                        let atr_values = crate::engine::indicators::atr(&candles, 14);
                        let current_atr = atr_values.last().copied().unwrap_or(0.0);
                        let stop_dist = (current_price - sl).abs();
                        let stop_dist_pct = (stop_dist / current_price) * 100.0;
                        let target_dist = (tp - current_price).abs();
                        let rr = if stop_dist > 0.0 { target_dist / stop_dist } else { 0.0 };
                        
                        let tier_label = if tf_tier.is_intraday() { "INTRADAY" } else { "SWING fallback" };
                        tracing::info!(
                            "📊 Local ATR stops [{}]: ATR={:.2}, SL mult={:.1}x, TP mult={:.1}x, Stop={:.1}%, R:R={:.2}",
                            tier_label, current_atr, atr_sl_mult, atr_tp_mult, stop_dist_pct, rr
                        );
                        
                        (sl, tp, current_atr, stop_dist_pct, rr)
                    }
                } else {
                    // Fallback to fixed pip stops (legacy)
                    let pip_value = match instrument {
                        "BTC_USD" => 1.0,
                        "XAU_USD" => 0.1,
                        _ => 0.1,
                    };
                    let (sl, tp) = if direction == "BUY" {
                        (
                            current_price - self.sl_pips * pip_value,
                            current_price + self.tp_pips * pip_value,
                        )
                    } else {
                        (
                            current_price + self.sl_pips * pip_value,
                            current_price - self.tp_pips * pip_value,
                        )
                    };
                    
                    let stop_dist = (current_price - sl).abs();
                    let stop_dist_pct = (stop_dist / current_price) * 100.0;
                    let target_dist = (tp - current_price).abs();
                    let rr = if stop_dist > 0.0 { target_dist / stop_dist } else { 0.0 };
                    
                    (sl, tp, 0.0, stop_dist_pct, rr)
                };
                
                // Log signal with MRATE and risk metrics
                let _ = log_activity(
                    &self.pool,
                    self.bot_id,
                    ActivityType::Signal,
                    &format!("🎯 {} signal: {} | ATR: {:.2} | Stop: {:.1}% | R:R: {:.2}", 
                        direction, signal_result.reason, atr_value, stop_distance_pct, rr_ratio),
                    Some(serde_json::json!({
                        "direction": direction,
                        "price": current_price,
                        "sl": sl_price,
                        "tp": tp_price,
                        "confidence": signal_result.confidence,
                        "mrate_lot_mult": mrate_lot_multiplier,
                        "adjusted_lot_size": adjusted_lot_size,
                        "atr": atr_value,
                        "stop_distance_pct": stop_distance_pct,
                        "rr_ratio": rr_ratio,
                        "use_atr_stops": use_atr_stops,
                    })),
                ).await;
                
                // Place the order with adjusted lot size
                match client.place_order(
                    instrument,
                    direction,
                    adjusted_lot_size,
                    Some(sl_price),
                    Some(tp_price),
                ).await {
                    Ok(order) => {
                        tracing::info!("✅ Order placed: {:?}", order);
                        let _ = log_activity(
                            &self.pool,
                            self.bot_id,
                            ActivityType::OrderPlaced,
                            &format!("✅ {} order placed @ {:.2}", direction, current_price),
                            Some(serde_json::json!({
                                "order_id": order.id,
                                "direction": direction,
                                "price": current_price,
                                "lot_size": self.lot_size,
                                "sl": sl_price,
                                "tp": tp_price,
                            })),
                        ).await;
                        
                        // Update bot stats
                        let _ = sqlx::query(
                            "UPDATE bots SET total_trades = total_trades + 1 WHERE id = $1"
                        )
                        .bind(self.bot_id)
                        .execute(&self.pool)
                        .await;
                        
                        // ═══════════════════════════════════════════════════════════════════
                        // TRADE CONTEXT CAPTURE - Store market conditions for learning
                        // ═══════════════════════════════════════════════════════════════════
                        let trade_context_result = self.capture_trade_context(
                            &candles,
                            &order.id,
                            instrument,
                            if direction == "BUY" { "LONG" } else { "SHORT" },
                            current_price,
                            adjusted_lot_size,
                            signal_result.confidence,
                            &signal_result.reason,
                            mrate_ref.cloned(),
                        ).await;
                        
                        if let Err(e) = trade_context_result {
                            tracing::warn!("Failed to capture trade context: {}", e);
                        } else {
                            tracing::info!("📊 Trade context captured for learning");
                        }
                        
                        // ═══════════════════════════════════════════════════════════════════
                        // RISK ENGINE - Record position for tracking
                        // ═══════════════════════════════════════════════════════════════════
                        let position = crate::risk::models::OpenPosition {
                            position_id: uuid::Uuid::new_v4(),  // Generate unique ID
                            bot_id: self.bot_id,
                            strategy_id: self.strategy_id,
                            instrument: instrument.to_string(),
                            direction: if direction == "BUY" { "LONG" } else { "SHORT" }.to_string(),
                            entry_price: current_price,
                            lot_size: adjusted_lot_size,
                            notional_value: proposed_notional,
                            opened_at: chrono::Utc::now(),
                        };
                        
                        self.risk_engine.record_position(position.clone()).await;
                        tracing::info!("🎯 Position recorded in risk engine: {} ({} @ {:.2})", 
                            position.position_id, direction, current_price);
                        
                        last_signal_time = Some(std::time::Instant::now());
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to place order: {}", e);
                        let _ = log_activity(
                            &self.pool,
                            self.bot_id,
                            ActivityType::Error,
                            &format!("❌ Order failed: {}", e),
                            None,
                        ).await;
                    }
                }
            }
        }
    }
    
    /// Fetch current MRATE output from shared state
    async fn fetch_mrate(&self) -> Option<MrateOutput> {
        get_current_mrate(&self.mrate_state).await
    }
    
    /// Capture trade context for learning/adaptation
    async fn capture_trade_context(
        &self,
        candles: &[Candle],
        external_trade_id: &str,
        symbol: &str,
        direction: &str,
        entry_price: f64,
        lot_size: f64,
        confidence: f64,
        reason: &str,
        mrate: Option<MrateOutput>,
    ) -> Result<(), String> {
        let n = candles.len();
        if n < 50 {
            return Err("Not enough candles for context".to_string());
        }
        let i = n - 1;
        
        // Calculate technical indicators
        let rsi_values = rsi(candles, 14);
        let (adx_values, _, _) = adx(candles, 14);
        let atr_values = atr(candles, 14);
        let ema_trend = ema(candles, 200);
        let ema_entry = ema(candles, 21);
        
        // Get indicator values at entry
        let rsi_val = rsi_values.get(i).copied();
        let adx_val = adx_values.get(i).copied();
        let atr_val = atr_values.get(i).copied();
        let ema_t = ema_trend.get(i).copied();
        let ema_e = ema_entry.get(i).copied();
        
        // Calculate volatility ratio (current ATR vs 50-period average)
        let volatility_ratio = if i >= 50 && atr_val.is_some() {
            let atr_avg: f64 = atr_values[i.saturating_sub(50)..i].iter().sum::<f64>() / 50.0;
            if atr_avg > 0.0 {
                Some(atr_val.unwrap() / atr_avg)
            } else {
                None
            }
        } else {
            None
        };
        
        // Calculate recent move in ATR terms
        let recent_move_atr = if i >= 3 && atr_val.is_some() && atr_val.unwrap() > 0.0 {
            let move_amt = (candles[i].close - candles[i - 3].close).abs();
            Some(move_amt / atr_val.unwrap())
        } else {
            None
        };
        
        // Build context
        let mut builder = TradeContextBuilder::new()
            .bot_id(self.bot_id)
            .strategy_id(self.strategy_id)
            .external_trade_id(external_trade_id.to_string())
            .symbol(symbol)
            .direction(direction)
            .entry_price(entry_price)
            .lot_size(lot_size)
            .signal(confidence, reason)
            .strategy_params(self.strategy_params.clone());
        
        // Add technical indicators if available
        if let (Some(r), Some(a), Some(t)) = (rsi_val, adx_val, atr_val) {
            builder = builder.technical_indicators(r, a, t);
        }
        
        if let (Some(et), Some(ee)) = (ema_t, ema_e) {
            builder = builder.ema_values(et, ee);
        }
        
        if let Some(vr) = volatility_ratio {
            builder = builder.volatility_ratio(vr);
        }
        
        if let Some(rm) = recent_move_atr {
            builder = builder.recent_move_atr(rm);
        }
        
        // Add MRATE context if available
        if let Some(m) = mrate {
            builder = builder.mrate(m);
        }
        
        // Build and insert
        let context = builder.build().map_err(|e| e.to_string())?;
        insert_trade_context(&self.pool, &context).await
            .map_err(|e| e.to_string())?;
        
        Ok(())
    }
    
    /// Sync closed trades from broker to update trade_context with exit data
    async fn sync_closed_trades(&self, client: &OandaClient) -> Result<(), String> {
        // Get recently closed trades from broker
        let closed_trades = client.get_closed_trades(20).await
            .map_err(|e| format!("Failed to fetch closed trades: {}", e))?;
        
        for trade in closed_trades {
            // Check if we have this trade in trade_context (by external_trade_id)
            // and if it hasn't been updated yet (closed_at is NULL)
            let needs_update: Option<(uuid::Uuid,)> = sqlx::query_as(
                "SELECT id FROM trade_context WHERE external_trade_id = $1 AND closed_at IS NULL AND bot_id = $2"
            )
            .bind(&trade.id)
            .bind(self.bot_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("DB query failed: {}", e))?;
            
            if let Some((context_id,)) = needs_update {
                // Parse trade data
                let exit_price: f64 = trade.average_close_price
                    .as_ref()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0.0);
                let pnl: f64 = trade.realized_pl
                    .as_ref()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0.0);
                
                // Determine exit reason from trade state
                let exit_reason = if trade.stop_loss_order.as_ref().and_then(|o| o.state.as_ref()).map(|s| s.as_str()) == Some("FILLED") {
                    "STOP_LOSS"
                } else if trade.take_profit_order.as_ref().and_then(|o| o.state.as_ref()).map(|s| s.as_str()) == Some("FILLED") {
                    "TAKE_PROFIT"
                } else {
                    "MANUAL_CLOSE"
                };
                
                let is_winner = pnl > 0.0;
                
                // Calculate trade duration
                let close_time = trade.close_time.as_ref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| t.with_timezone(&chrono::Utc));
                let open_time = chrono::DateTime::parse_from_rfc3339(&trade.open_time)
                    .ok()
                    .map(|t| t.with_timezone(&chrono::Utc));
                let duration_minutes = match (open_time, close_time) {
                    (Some(o), Some(c)) => Some((c - o).num_minutes() as i32),
                    _ => None,
                };
                
                // Update trade_context with exit data
                let result = sqlx::query(
                    r#"
                    UPDATE trade_context SET
                        exit_price = $1,
                        pnl = $2,
                        exit_reason = $3,
                        is_winner = $4,
                        trade_duration_minutes = $5,
                        closed_at = $6
                    WHERE id = $7
                    "#
                )
                .bind(exit_price)
                .bind(pnl)
                .bind(exit_reason)
                .bind(is_winner)
                .bind(duration_minutes)
                .bind(close_time)
                .bind(context_id)
                .execute(&self.pool)
                .await;
                
                match result {
                    Ok(_) => {
                        tracing::info!(
                            "📊 Trade {} closed: {} @ {:.2}, PnL: {:.2} ({})",
                            trade.id, exit_reason, exit_price, pnl,
                            if is_winner { "WIN" } else { "LOSS" }
                        );
                        
                        // Update bot's total PnL
                        let _ = sqlx::query(
                            "UPDATE bots SET total_pnl = total_pnl + $1 WHERE id = $2"
                        )
                        .bind(pnl)
                        .bind(self.bot_id)
                        .execute(&self.pool)
                        .await;
                        
                        // ══════════════════════════════════════════
                        // RISK ENGINE - Update closed position
                        // ══════════════════════════════════════════
                        // Try to find and close the position in risk engine
                        // We'll search by instrument since we may not have the exact position_id
                        self.risk_engine.record_position_closed_by_instrument(
                            &trade.instrument,
                            self.bot_id,
                            pnl,
                        ).await;
                        
                        tracing::info!("🔴 Position closed in risk engine: {} (PnL: {:.2})", 
                            trade.instrument, pnl);
                        
                        // ══════════════════════════════════════════
                        // ENSEMBLE LEARNING - Record trade outcome
                        // ══════════════════════════════════════════
                        self.record_ensemble_learning(
                            &trade.instrument,
                            is_winner,
                            pnl,
                            context_id,
                        ).await;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to update trade context: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Reconcile trade_context with actual OANDA positions
    /// This catches trades that closed but weren't synced (manual closes, restarts, etc.)
    async fn reconcile_positions(&self, client: &OandaClient) -> Result<(), String> {
        // Get all "open" positions in our DB for this bot
        let db_open_positions: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT id, external_trade_id FROM trade_context WHERE bot_id = $1 AND closed_at IS NULL"
        )
        .bind(self.bot_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("DB query failed: {}", e))?;
        
        if db_open_positions.is_empty() {
            return Ok(());
        }
        
        // Get actual open trades from OANDA
        let oanda_open_trades = client.get_open_trades().await
            .map_err(|e| format!("Failed to fetch OANDA open trades: {}", e))?;
        
        let oanda_trade_ids: std::collections::HashSet<String> = oanda_open_trades
            .iter()
            .map(|t| t.id.clone())
            .collect();
        
        // Find orphaned records (in DB but not in OANDA)
        let orphaned: Vec<_> = db_open_positions
            .iter()
            .filter(|(_, ext_id)| !oanda_trade_ids.contains(ext_id))
            .collect();
        
        if orphaned.is_empty() {
            return Ok(());
        }
        
        tracing::info!("🔄 Found {} orphaned trade_context records, reconciling...", orphaned.len());
        
        // Fetch recent closed trades to get actual exit data
        let closed_trades = client.get_closed_trades(100).await
            .map_err(|e| format!("Failed to fetch closed trades: {}", e))?;
        
        for (context_id, external_trade_id) in orphaned {
            // Try to find this trade in closed trades
            if let Some(closed_trade) = closed_trades.iter().find(|t| &t.id == external_trade_id) {
                // Found it - update with actual exit data
                let exit_price: f64 = closed_trade.average_close_price
                    .as_ref()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0.0);
                let pnl: f64 = closed_trade.realized_pl
                    .as_ref()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0.0);
                
                let exit_reason = if closed_trade.stop_loss_order.as_ref().and_then(|o| o.state.as_ref()).map(|s| s.as_str()) == Some("FILLED") {
                    "STOP_LOSS"
                } else if closed_trade.take_profit_order.as_ref().and_then(|o| o.state.as_ref()).map(|s| s.as_str()) == Some("FILLED") {
                    "TAKE_PROFIT"
                } else {
                    "MANUAL_CLOSE"
                };
                
                let close_time = closed_trade.close_time.as_ref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|t| t.with_timezone(&chrono::Utc));
                
                let _ = sqlx::query(
                    r#"UPDATE trade_context SET
                        exit_price = $1,
                        pnl = $2,
                        exit_reason = $3,
                        is_winner = $4,
                        closed_at = $5
                    WHERE id = $6"#
                )
                .bind(exit_price)
                .bind(pnl)
                .bind(exit_reason)
                .bind(pnl > 0.0)
                .bind(close_time)
                .bind(context_id)
                .execute(&self.pool)
                .await;
                
                tracing::info!("📊 Reconciled trade {}: {} @ {:.2}, PnL: {:.2}", 
                    external_trade_id, exit_reason, exit_price, pnl);
            } else {
                // Trade not found in recent history - mark as unknown close
                let _ = sqlx::query(
                    r#"UPDATE trade_context SET
                        exit_reason = 'UNKNOWN',
                        closed_at = NOW()
                    WHERE id = $1"#
                )
                .bind(context_id)
                .execute(&self.pool)
                .await;
                
                tracing::warn!("⚠️ Trade {} not found in OANDA history, marked as closed", external_trade_id);
            }
        }
        
        Ok(())
    }
    
    /// Record trade outcome for cross-bot ensemble learning
    async fn record_ensemble_learning(
        &self,
        instrument: &str,
        is_winner: bool,
        pnl: f64,
        trade_context_id: Uuid,
    ) {
        use crate::analytics::ensemble::{EnsembleEngine, ClusterId, MarketPattern};
        
        // Get current regime from MRATE
        let mrate = self.fetch_mrate().await.unwrap_or_else(default_mrate_output);
        let regime = mrate.regime.as_str().to_string();
        
        // Determine session
        let hour = chrono::Utc::now().hour();
        let session = match hour {
            0..=7 => "ASIAN",
            8..=11 => "LONDON",
            12..=15 => "OVERLAP",
            16..=21 => "NEW_YORK",
            _ => "OFF_HOURS",
        }.to_string();
        
        // Determine strategy category for clustering
        let category = self.mrate_category
            .map(|c| c.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        
        let cluster_id = ClusterId::new(&category, instrument);
        
        // Create pattern from current conditions
        let pattern = MarketPattern {
            regime: regime.clone(),
            session: Some(session),
            rsi_zone: "neutral".to_string(), // Simplified - could fetch from trade_context
            volatility_level: "normal".to_string(),
            trend_strength: "moderate".to_string(),
        };
        
        let outcome = if is_winner { "win" } else { "loss" };
        
        let engine = EnsembleEngine::new(self.pool.clone());
        if let Err(e) = engine.record_trade_outcome(
            &cluster_id,
            &pattern,
            outcome,
            0.7, // Could get actual confidence from trade_context
            pnl,
            Some(trade_context_id),
            self.bot_id,
        ).await {
            tracing::warn!("Failed to record ensemble learning: {}", e);
        } else {
            tracing::info!(
                "🧠 Ensemble: Recorded {} for {} cluster (regime: {}, pnl: {:.2})",
                outcome, cluster_id.as_string(), regime, pnl
            );
        }
    }
}
