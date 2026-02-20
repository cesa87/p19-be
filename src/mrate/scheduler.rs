//! MRATE Scheduler
//! 
//! Background task that updates MRATE every 5 minutes

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{info, error, warn};
use uuid::Uuid;

use crate::broker::OandaClient;
use crate::config::Config;
use crate::analytics::adaptation::{AdaptationEngine, AdaptationConfig, save_adaptation};
use crate::analytics::correlation::CorrelationEngine;
use super::activity_log::{log_feed_success, log_feed_error, log_regime_change, log_score_update, log_decision};

use super::engine::MrateEngine;
use super::models::{
    MrateOutput, MrateSnapshot, Regime, StrategyWeights, FavoredInstrument,
    InstrumentScores, InstrumentScore, InstrumentRecommendation,
};
use super::orchestrator::Orchestrator;
use super::thresholds::DynamicThresholds;

/// Shared state for MRATE output
pub type MrateState = Arc<RwLock<Option<MrateOutput>>>;

/// Create a new MRATE state holder
pub fn create_mrate_state() -> MrateState {
    Arc::new(RwLock::new(None))
}

/// Shared engine type for use across scheduler and API
pub type SharedMrateEngine = Arc<MrateEngine>;

/// Create a new shared MRATE engine
pub fn create_mrate_engine() -> SharedMrateEngine {
    Arc::new(MrateEngine::new())
}

/// MRATE Scheduler - runs engine every 5 minutes
pub struct MrateScheduler {
    engine: SharedMrateEngine,
    state: MrateState,
    pool: sqlx::PgPool,
    config: Config,
    persist_history: bool,
}

impl MrateScheduler {
    pub fn new(
        engine: SharedMrateEngine,
        state: MrateState,
        pool: sqlx::PgPool,
        config: Config,
        persist_history: bool,
    ) -> Self {
        Self {
            engine,
            state,
            pool,
            config,
            persist_history,
        }
    }
    
    /// Start the scheduler loop
    pub async fn run(self) {
        info!("🚀 MRATE Scheduler starting (adaptive interval)");
        
        // Validate FRED API key if gold trading is enabled
        self.validate_fred_config();
        
        // Run immediately on startup
        self.update().await;
        
        // Adaptive loop — interval depends on proximity to high-impact events
        loop {
            let interval_secs = self.next_interval_secs().await;
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            self.update().await;
        }
    }
    
    /// Determine next interval based on event proximity
    /// Near events: 60s, same-day events: 120s, otherwise: 300s
    async fn next_interval_secs(&self) -> u64 {
        let state = self.state.read().await;
        let hours = state.as_ref()
            .and_then(|o| o.inputs.as_ref())
            .and_then(|i| i.hours_to_next_high_impact);
        
        match hours {
            Some(h) if h <= 4.0 => {
                info!("⏱️ High-impact event in {:.1}h — MRATE interval: 60s", h);
                60
            }
            Some(h) if h <= 12.0 => {
                info!("⏱️ High-impact event in {:.1}h — MRATE interval: 120s", h);
                120
            }
            _ => 300,
        }
    }
    
    /// Run a single update cycle
    async fn update(&self) {
        // Create OANDA client if credentials available
        let oanda = self.create_oanda_client();
        
        // Calculate new MRATE output
        let mut output = self.engine.calculate(oanda.as_ref()).await;
        
        // Enrich with trade feedback from open positions
        if let Some(ref client) = oanda {
            self.inject_trade_feedback(&mut output, client).await;
        }
        
        // Apply locked directions (institutional daily close approach)
        // This overrides the live price vs EMA calculation with the locked direction from NY close
        self.apply_locked_directions(&mut output).await;
        
        // Alert if in degraded mode
        if output.degraded_mode {
            warn!(
                "⚠️ MRATE DEGRADED MODE: {} | Healthy feeds: {}/4",
                output.data_health.status_summary(),
                output.data_health.healthy_feed_count()
            );
        }
        
        // Update shared state
        {
            let mut state = self.state.write().await;
            *state = Some(output.clone());
        }
        
        // Persist to database if enabled
        if self.persist_history {
            if let Err(e) = self.save_snapshot(&output).await {
                error!("Failed to save MRATE snapshot: {}", e);
            }
        }
        
        // Run orchestrator to auto-enable/disable bots based on current conditions
        // Skip when MRATE is globally disabled — bots trade freely
        let mrate_enabled = crate::mrate::is_mrate_enabled(&self.pool).await;
        if !mrate_enabled {
            tracing::debug!("MRATE disabled — skipping orchestrator auto-enable/disable");
        }
        let orchestrator = Orchestrator::new(self.pool.clone());
        if mrate_enabled {
        match orchestrator.get_recommendations(&output).await {
            Ok(response) => {
                let s = &response.summary;
                if s.mismatches > 0 {
                    info!(
                        "🎯 Orchestrator: {} activate, {} monitor, {} pause ({} mismatches adjusted)",
                        s.activate_count, s.monitor_count, s.pause_count, s.mismatches
                    );
                }
                if !response.coverage_gaps.is_empty() {
                    for gap in &response.coverage_gaps {
                        warn!(
                            "⚠️ Coverage gap: {} regime / {} session — best score {:.0}. {}",
                            gap.regime, gap.session, gap.best_score, gap.suggestion
                        );
                    }
                }
            }
            Err(e) => {
                error!("Failed to run orchestrator: {}", e);
            }
        }
        } // end if mrate_enabled
        
        // ═══════════════════════════════════════════════════════════════════
        // LOCKED DIRECTION UPDATE - Update at NY close (10pm GMT)
        // ═══════════════════════════════════════════════════════════════════
        if super::locked_direction::is_ny_close_window() {
            if let Some(ref client) = oanda {
                match super::locked_direction::update_all_directions_at_ny_close(&self.pool, client).await {
                    Ok(count) => {
                        if count > 0 {
                            info!("🔒 NY Close: Updated {} instrument directions", count);
                        }
                    }
                    Err(e) => {
                        error!("Failed to update locked directions: {}", e);
                    }
                }
            }
        }
        
        // ═══════════════════════════════════════════════════════════════════
        // PERIODIC ADAPTATION - analyze performance and propose adjustments
        // ═══════════════════════════════════════════════════════════════════
        self.run_adaptation_cycle().await;
        
        // ═══════════════════════════════════════════════════════════════════
        // AI BRAIN DATA - Update correlation matrix (every ~30 min)
        // ═══════════════════════════════════════════════════════════════════
        self.run_ai_brain_updates().await;
        
        let status_indicator = if output.degraded_mode { "⚠️" } else { "📊" };
        info!(
            "{} MRATE updated: {} | Liq={:.0} Risk={:.0} Unc={:.0} | Mult={:.2}x{}",
            status_indicator,
            output.regime.as_str(),
            output.liquidity_score,
            output.risk_score,
            output.uncertainty_score,
            output.risk_multiplier,
            if output.degraded_mode { " [DEGRADED]" } else { "" }
        );
        
        // ═══════════════════════════════════════════════════════════════════
        // ACTIVITY LOGGING - Record what MRATE is doing for UI transparency
        // ═══════════════════════════════════════════════════════════════════
        self.log_mrate_activities(&output).await;
    }
    
    /// Log MRATE activities to the database for UI transparency
    async fn log_mrate_activities(&self, output: &MrateOutput) {
        // Log score update with key values
        if let Some(inputs) = &output.inputs {
            let details = serde_json::json!({
                "regime": output.regime.as_str(),
                "liquidity_score": output.liquidity_score,
                "risk_score": output.risk_score,
                "uncertainty_score": output.uncertainty_score,
                "risk_multiplier": output.risk_multiplier,
                "vix": inputs.vix_level,
                "dxy": inputs.dxy_level,
                "fear_greed": inputs.fear_greed_index,
                "degraded": output.degraded_mode,
            });
            
            let message = format!(
                "Scores: Liq={:.0} Risk={:.0} Unc={:.0} | Regime: {} | Mult: {:.2}x",
                output.liquidity_score, output.risk_score, output.uncertainty_score,
                output.regime.as_str(), output.risk_multiplier
            );
            
            log_score_update(&self.pool, &message, Some(details)).await;
        }
        
        // Log individual feed statuses
        let health = &output.data_health;
        
        if health.polymarket_ok {
            if let Some(inputs) = &output.inputs {
                let details = serde_json::json!({
                    "rate_cut_prob": inputs.rate_cut_probability,
                    "btc_sentiment": inputs.btc_sentiment,
                    "inflation_exp": inputs.inflation_expectations,
                });
                log_feed_success(&self.pool, "polymarket", 
                    &format!("Fed Cut: {:.0}%, BTC: {:.0}%", 
                        inputs.rate_cut_probability.unwrap_or(0.0),
                        inputs.btc_sentiment.unwrap_or(0.0)), 
                    Some(details)).await;
            }
        }
        
        if health.vix_ok {
            if let Some(inputs) = &output.inputs {
                log_feed_success(&self.pool, "vix", 
                    &format!("VIX: {:.1}", inputs.vix_level.unwrap_or(0.0)), 
                    Some(serde_json::json!({"vix": inputs.vix_level}))).await;
            }
        }
        
        if health.dxy_ok {
            if let Some(inputs) = &output.inputs {
                log_feed_success(&self.pool, "dxy", 
                    &format!("DXY: {:.2}", inputs.dxy_level.unwrap_or(0.0)), 
                    Some(serde_json::json!({"dxy": inputs.dxy_level}))).await;
            }
        }
        
        if health.fear_greed_ok {
            if let Some(inputs) = &output.inputs {
                let fg = inputs.fear_greed_index.unwrap_or(50.0) as i32;
                let label = match fg {
                    0..=20 => "Extreme Fear",
                    21..=40 => "Fear",
                    41..=60 => "Neutral",
                    61..=80 => "Greed",
                    _ => "Extreme Greed",
                };
                log_feed_success(&self.pool, "fear_greed", 
                    &format!("Fear & Greed: {} ({})", fg, label), 
                    Some(serde_json::json!({"fear_greed": fg, "label": label}))).await;
            }
        }
        
        if health.fred_ok {
            if let Some(inputs) = &output.inputs {
                log_feed_success(&self.pool, "fred", 
                    &format!("10Y: {:.2}%, Real: {:.2}%", 
                        inputs.yield_10y.unwrap_or(0.0),
                        inputs.real_yield_10y.unwrap_or(0.0)), 
                    Some(serde_json::json!({
                        "yield_10y": inputs.yield_10y,
                        "real_yield": inputs.real_yield_10y
                    }))).await;
            }
        }
        
        // Log Reddit sentiment if available
        if let Some(inputs) = &output.inputs {
            if inputs.reddit_crypto_sentiment.is_some() {
                log_feed_success(&self.pool, "reddit", 
                    &format!("Reddit: Crypto {:.0}%, BTC {:.0}%, Gold {:.0}%", 
                        inputs.reddit_crypto_sentiment.unwrap_or(0.0) * 100.0,
                        inputs.reddit_btc_sentiment.unwrap_or(0.0) * 100.0,
                        inputs.reddit_gold_sentiment.unwrap_or(0.0) * 100.0), 
                    Some(serde_json::json!({
                        "crypto": inputs.reddit_crypto_sentiment,
                        "btc": inputs.reddit_btc_sentiment,
                        "gold": inputs.reddit_gold_sentiment
                    }))).await;
            }
        }
        
        // Log errors for failed feeds
        if !health.polymarket_ok {
            log_feed_error(&self.pool, "polymarket", "Failed to fetch Polymarket data").await;
        }
        if !health.vix_ok {
            log_feed_error(&self.pool, "vix", "Failed to fetch VIX data").await;
        }
        if !health.dxy_ok {
            log_feed_error(&self.pool, "dxy", "Failed to fetch DXY data").await;
        }
        if !health.fear_greed_ok {
            log_feed_error(&self.pool, "fear_greed", "Failed to fetch Fear & Greed data").await;
        }
    }
    
    /// Validate FRED API configuration
    fn validate_fred_config(&self) {
        if self.config.require_fred_for_gold {
            if std::env::var("FRED_API_KEY").is_err() {
                error!(
                    "⚠️ FRED_API_KEY is not set but require_fred_for_gold=true. \
                    Gold trading recommendations will be disabled. \
                    Get a free API key from https://fred.stlouisfed.org/docs/api/api_key.html"
                );
            } else {
                info!("✓ FRED API key configured - real yield data available for gold trading");
            }
        }
    }
    
    /// Create OANDA client from config
    fn create_oanda_client(&self) -> Option<OandaClient> {
        let api_token = self.config.oanda_api_token.as_ref()?;
        let account_id = self.config.oanda_account_id.as_ref()?;
        
        OandaClient::new(api_token, account_id, self.config.oanda_practice).ok()
    }
    
    /// Run adaptation engine for active bots with enough trade history
    async fn run_adaptation_cycle(&self) {
        let config = AdaptationConfig {
            auto_apply_enabled: true,
            auto_apply_confidence: 0.80,
            ..AdaptationConfig::default()
        };
        let engine = AdaptationEngine::new(config.clone());
        
        // Fetch active bots with their strategy IDs
        let bots = match sqlx::query_as::<_, (Uuid, Uuid, String)>(
            r#"
            SELECT b.id, b.strategy_id, b.name
            FROM bots b
            WHERE b.is_active = true AND b.strategy_id IS NOT NULL
            "#
        )
        .fetch_all(&self.pool)
        .await {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to fetch bots for adaptation: {}", e);
                return;
            }
        };
        
        let mut total_proposals = 0usize;
        let mut auto_applied = 0usize;
        
        for (bot_id, strategy_id, bot_name) in &bots {
            match engine.analyze_and_propose(&self.pool, *bot_id, *strategy_id).await {
                Ok(proposals) if !proposals.is_empty() => {
                    for proposal in &proposals {
                        total_proposals += 1;
                        let should_auto_apply = config.auto_apply_enabled 
                            && proposal.confidence >= config.auto_apply_confidence;
                        
                        match save_adaptation(&self.pool, *bot_id, *strategy_id, proposal, should_auto_apply).await {
                            Ok(adaptation) => {
                                if should_auto_apply {
                                    auto_applied += 1;
                                    info!(
                                        "🧠 Auto-applied adaptation for '{}': {} ({:.0}% confidence)",
                                        bot_name, proposal.reason, proposal.confidence * 100.0
                                    );
                                }
                            }
                            Err(e) => {
                                // Likely duplicate — ignore gracefully
                                tracing::debug!("Adaptation save skipped for {}: {}", bot_name, e);
                            }
                        }
                    }
                }
                Ok(_) => {} // No proposals — bot doesn't have enough data yet
                Err(e) => {
                    tracing::debug!("Adaptation analysis failed for {}: {}", bot_name, e);
                }
            }
        }
        
        if total_proposals > 0 {
            info!(
                "🧠 Adaptation cycle: {} proposals, {} auto-applied across {} active bots",
                total_proposals, auto_applied, bots.len()
            );
        }
    }
    
    /// Inject trade feedback from open positions into MRATE inputs
    /// This allows MRATE to be aware of current exposure when calculating regime
    async fn inject_trade_feedback(&self, output: &mut MrateOutput, oanda: &OandaClient) {
        use crate::analytics::trade_analyzer::TradeAnalyzer;
        
        // Analyze open trades
        let result = TradeAnalyzer::analyze_open_trades(&self.pool, oanda, output).await;
        
        match result {
            Ok((analyses, insights)) => {
                // Inject into MRATE inputs
                if let Some(ref mut inputs) = output.inputs {
                    inputs.has_open_positions = insights.total_open_trades > 0;
                    inputs.open_trade_count = insights.total_open_trades as i32;
                    inputs.total_unrealized_pnl = Some(insights.total_unrealized_pnl);
                    inputs.net_position_bias = Some(insights.net_direction_bias);
                    inputs.position_regime_alignment = Some(insights.regime_alignment_pct);
                    inputs.positions_at_risk = insights.trades_needing_attention as i32;
                    
                    // Calculate stress score
                    let stress = if insights.total_open_trades > 0 {
                        let attention_ratio = insights.trades_needing_attention as f64 / insights.total_open_trades as f64;
                        let concentration = insights.position_concentration;
                        let loss_stress = if insights.total_unrealized_pnl < 0.0 { 0.2 } else { 0.0 };
                        ((attention_ratio * 0.4 + concentration * 0.3 + loss_stress) * 100.0).min(100.0)
                    } else {
                        0.0
                    };
                    inputs.position_stress_score = Some(stress);
                    
                    // Log if there are positions needing attention
                    if insights.trades_needing_attention > 0 {
                        warn!(
                            "⚠️ {} of {} positions need attention (regime alignment: {:.0}%)",
                            insights.trades_needing_attention,
                            insights.total_open_trades,
                            insights.regime_alignment_pct
                        );
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Trade feedback unavailable: {}", e);
            }
        }
    }
    
    /// Run AI Brain data updates (correlation matrix, event learning)
    /// These run less frequently than MRATE updates
    async fn run_ai_brain_updates(&self) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static CYCLE_COUNTER: AtomicU32 = AtomicU32::new(0);
        
        let cycle = CYCLE_COUNTER.fetch_add(1, Ordering::Relaxed);
        
        // Update correlation matrix every 6 cycles (~30 min at 5 min intervals)
        if cycle % 6 == 0 {
            let correlation_engine = CorrelationEngine::new(self.pool.clone());
            match correlation_engine.update_matrix().await {
                Ok(count) => {
                    info!("📊 AI Brain: Updated {} correlation pairs", count);
                }
                Err(e) => {
                    warn!("Failed to update correlation matrix: {}", e);
                }
            }
        }
    }
    
    /// Apply locked directions from database to MRATE output
    /// This overrides the live price vs EMA calculation with the institutional daily close approach
    async fn apply_locked_directions(&self, output: &mut MrateOutput) {
        use super::locked_direction::{get_all_locked_directions, LockedDirection};
        use super::models::TradingDirection;
        
        match get_all_locked_directions(&self.pool).await {
            Ok(directions) => {
                for locked in directions {
                    let dir = locked.trading_direction();
                    
                    // Apply to the appropriate instrument score
                    match locked.instrument.as_str() {
                        "XAU_USD" => {
                            if output.instrument_scores.gold.trading_direction != dir {
                                info!("🔒 Gold: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.gold.trading_direction = dir;
                        }
                        "BTC_USD" => {
                            if output.instrument_scores.bitcoin.trading_direction != dir {
                                info!("🔒 Bitcoin: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.bitcoin.trading_direction = dir;
                        }
                        "EUR_USD" => {
                            if output.instrument_scores.eur_usd.trading_direction != dir {
                                info!("🔒 EUR/USD: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.eur_usd.trading_direction = dir;
                        }
                        "USD_JPY" => {
                            if output.instrument_scores.usd_jpy.trading_direction != dir {
                                info!("🔒 USD/JPY: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.usd_jpy.trading_direction = dir;
                        }
                        "WTICO_USD" => {
                            if output.instrument_scores.wti_oil.trading_direction != dir {
                                info!("🔒 WTI Oil: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.wti_oil.trading_direction = dir;
                        }
                        "NATGAS_USD" => {
                            if output.instrument_scores.natural_gas.trading_direction != dir {
                                info!("🔒 Natural Gas: Overriding live direction with locked {}", locked.direction);
                            }
                            output.instrument_scores.natural_gas.trading_direction = dir;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch locked directions: {} - using live calculation", e);
            }
        }
    }
    
    /// Save snapshot to database
    async fn save_snapshot(&self, output: &MrateOutput) -> Result<(), sqlx::Error> {
        let weights_json = serde_json::to_value(&output.strategy_weights)
            .unwrap_or(serde_json::json!({}));
        let inputs_json = output.inputs.as_ref()
            .map(|i| serde_json::to_value(i).unwrap_or(serde_json::json!({})));
        let instrument_scores_json = serde_json::to_value(&output.instrument_scores)
            .unwrap_or(serde_json::json!({}));
        
        // Save to mrate_snapshots (main MRATE history)
        sqlx::query(
            r#"
            INSERT INTO mrate_snapshots (
                id, timestamp, regime, liquidity_score, risk_score, 
                uncertainty_score, risk_multiplier, strategy_weights, raw_inputs,
                instrument_scores
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(Uuid::new_v4())
        .bind(output.timestamp)
        .bind(output.regime.as_str())
        .bind(output.liquidity_score as f32)
        .bind(output.risk_score as f32)
        .bind(output.uncertainty_score as f32)
        .bind(output.risk_multiplier as f32)
        .bind(weights_json)
        .bind(inputs_json)
        .bind(instrument_scores_json)
        .execute(&self.pool)
        .await?;
        
        // Also save to regime_snapshots (for AI Brain regime predictor)
        if let Some(inputs) = &output.inputs {
            let _ = sqlx::query(
                r#"
                INSERT INTO regime_snapshots (
                    id, timestamp, regime, liquidity_score, risk_score, 
                    uncertainty_score, liquidity_momentum, risk_momentum,
                    vix_level, dxy_level, fear_greed_index, btc_trend, sp500_trend
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#
            )
            .bind(Uuid::new_v4())
            .bind(output.timestamp)
            .bind(output.regime.as_str())
            .bind(output.liquidity_score as f32)
            .bind(output.risk_score as f32)
            .bind(output.uncertainty_score as f32)
            .bind(output.liquidity_momentum as f32)
            .bind(output.risk_momentum as f32)
            .bind(inputs.vix_level.map(|v| v as f32))
            .bind(inputs.dxy_level.map(|v| v as f32))
            .bind(inputs.fear_greed_index.map(|v| v as f32))
            .bind(inputs.btc_trend.as_ref().map(|t| format!("{:?}", t)))
            .bind(inputs.sp500_trend.as_ref().map(|t| format!("{:?}", t)))
            .execute(&self.pool)
            .await;
        }
        
        // Also save to sentiment_signals (for AI Brain sentiment dashboard)
        if let Some(inputs) = &output.inputs {
            // Count active bots
            let active_bots: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM bots WHERE is_active = true"
            )
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0) as i32;
            
            // Calculate contrarian signal
            let contrarian_signal = if let Some(fg) = inputs.fear_greed_index {
                if fg < 20.0 { "extreme_fear" }
                else if fg > 80.0 { "extreme_greed" }
                else { "neutral" }
            } else { "neutral" };
            
            // Calculate confidence multiplier from sentiment
            let conf_mult = if let Some(fg) = inputs.fear_greed_index {
                if fg < 20.0 || fg > 80.0 { 1.15 } else { 1.0 }
            } else { 1.0 };
            
            let _ = sqlx::query(
                r#"
                INSERT INTO sentiment_signals (
                    id, timestamp, fear_greed_index, reddit_crypto, reddit_btc, reddit_gold,
                    contrarian_signal, confidence_multiplier, regime, active_bots_count
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#
            )
            .bind(Uuid::new_v4())
            .bind(output.timestamp)
            .bind(inputs.fear_greed_index.map(|v| v as f32))
            .bind(inputs.reddit_crypto_sentiment.map(|v| v as f32))
            .bind(inputs.reddit_btc_sentiment.map(|v| v as f32))
            .bind(inputs.reddit_gold_sentiment.map(|v| v as f32))
            .bind(contrarian_signal)
            .bind(conf_mult as f32)
            .bind(output.regime.as_str())
            .bind(active_bots)
            .execute(&self.pool)
            .await;
        }
        
        Ok(())
    }
}

/// Get current MRATE output from state
pub async fn get_current_mrate(state: &MrateState) -> Option<MrateOutput> {
    let guard = state.read().await;
    guard.clone()
}

/// Get default MRATE output when none calculated yet
pub fn default_mrate_output() -> MrateOutput {
    use super::models::{DataHealth, TrendDirection, PriceRegime};
    
    // Create default neutral scores for all instruments
    let default_score = |symbol: &str| InstrumentScore {
        symbol: symbol.to_string(),
        score: 50.0,
        recommendation: InstrumentRecommendation::Neutral,
        factors: vec!["Awaiting data".to_string()],
        trend_direction: TrendDirection::Flat,
        trend_strength: 20.0,
        price_regime: PriceRegime::Ranging,
        technicals: super::models::InstrumentTechnicals::default(),
        trading_direction: super::models::TradingDirection::Neutral,
        best_strategies: vec![],
        mtf_data: None, // No MTF data until first calculation
        d1_atr: 0.0,
        recommended_stop_distance: 0.0,
        recommended_tp_distance: 0.0,
        stop_atr_multiplier: 2.0,
        ema_200: 0.0,
        current_price: 0.0,
    };
    
    MrateOutput {
        timestamp: chrono::Utc::now(),
        regime: Regime::Choppy,
        liquidity_score: 50.0,
        risk_score: 50.0,
        uncertainty_score: 50.0,
        risk_multiplier: 1.0,
        strategy_weights: StrategyWeights::for_regime(Regime::Choppy),
        favored_instrument: FavoredInstrument::Neither,
        instrument_scores: InstrumentScores {
            gold: default_score("XAU_USD"),
            bitcoin: default_score("BTC_USD"),
            eur_usd: default_score("EUR_USD"),
            usd_jpy: default_score("USD_JPY"),
            wti_oil: default_score("WTICO_USD"),
            natural_gas: default_score("NATGAS_USD"),
        },
        thresholds: DynamicThresholds::default(),
        regime_confidence: 0.0,
        proposed_regime: Regime::Choppy,
        regime_change_pending: false,
        liquidity_momentum: 0.0,
        risk_momentum: 0.0,
        data_health: DataHealth::default(),
        degraded_mode: true, // Default to degraded until first calculation
        inputs: None,
    }
}
