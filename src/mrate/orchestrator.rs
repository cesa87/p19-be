//! MRATE Strategy Orchestrator
//! 
//! Matches current market conditions to bot profiles and recommends
//! which bots should be active, paused, or monitored.

use chrono::{DateTime, Utc, Timelike};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::{MrateOutput, Regime, StrategyWeights};
use super::models::{StrategyCategory, InstrumentScores, InstrumentScore, TradingDirection, PriceRegime};
use crate::analytics::learning::{query_historical_performance, performance_score_adjustment};

/// Action recommendation for a bot
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecommendedAction {
    /// Score > 70: Conditions are favorable
    Activate,
    /// Score 40-70: Marginal conditions
    Monitor,
    /// Score < 40: Unfavorable conditions
    Pause,
}

impl RecommendedAction {
    pub fn from_score(score: f64) -> Self {
        if score >= 70.0 {
            RecommendedAction::Activate
        } else if score >= 40.0 {
            RecommendedAction::Monitor
        } else {
            RecommendedAction::Pause
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            RecommendedAction::Activate => "ACTIVATE",
            RecommendedAction::Monitor => "MONITOR",
            RecommendedAction::Pause => "PAUSE",
        }
    }
}

/// Strategy profile defining optimal trading conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyProfile {
    pub preferred_regimes: Vec<String>,
    pub best_sessions: Vec<String>,
    pub hours_utc: HoursRange,
    pub volatility_pref: String,
    pub min_adx: f64,
    pub rsi_range: RsiRange,
    pub min_mrate_weight: f64,
    pub adaptive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoursRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiRange {
    pub min: f64,
    pub max: f64,
}

impl Default for StrategyProfile {
    fn default() -> Self {
        Self {
            preferred_regimes: vec!["TREND".to_string(), "CHOPPY".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 30.0, max: 70.0 },
            min_mrate_weight: 0.4,
            adaptive: false,
        }
    }
}

/// Current market conditions for scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConditions {
    pub regime: Regime,
    pub session: String,
    pub hour_utc: u32,
    pub volatility_level: String,
    pub vix_level: Option<f64>,
    pub strategy_weights: StrategyWeights,
    pub risk_score: f64,
    pub liquidity_score: f64,
    /// Per-instrument scores from MRATE (used for instrument-aware orchestration)
    pub instrument_scores: InstrumentScores,
}

impl MarketConditions {
    /// Derive conditions from MRATE output
    pub fn from_mrate(mrate: &MrateOutput) -> Self {
        let now = Utc::now();
        let hour = now.hour();
        
        // Determine session from hour
        let session = match hour {
            0..=7 => "ASIAN",
            8..=11 => "LONDON",
            12..=15 => "OVERLAP",
            16..=21 => "NEW_YORK",
            _ => "OFF_HOURS",
        }.to_string();
        
        // Determine volatility level from VIX
        let volatility_level = match mrate.inputs.as_ref().and_then(|i| i.vix_level) {
            Some(vix) if vix > 25.0 => "high",
            Some(vix) if vix < 15.0 => "low",
            _ => "normal",
        }.to_string();
        
        Self {
            regime: mrate.regime,
            session,
            hour_utc: hour,
            volatility_level,
            vix_level: mrate.inputs.as_ref().and_then(|i| i.vix_level),
            strategy_weights: mrate.strategy_weights.clone(),
            risk_score: mrate.risk_score,
            liquidity_score: mrate.liquidity_score,
            instrument_scores: mrate.instrument_scores.clone(),
        }
    }
}

/// Individual bot recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRecommendation {
    pub bot_id: Uuid,
    pub bot_name: String,
    pub strategy_type: String,
    pub score: f64,
    pub action: RecommendedAction,
    pub reasons: Vec<String>,
    pub is_currently_enabled: bool,
    pub mismatch: bool, // True if action != current state
}

/// Gap in market coverage where no strategy scores well
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    pub regime: String,
    pub session: String,
    pub hour_utc: u32,
    pub volatility_level: String,
    pub best_score: f64,
    pub suggestion: String,
}

/// Full orchestrator response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResponse {
    pub timestamp: DateTime<Utc>,
    pub conditions: MarketConditions,
    pub recommendations: Vec<BotRecommendation>,
    pub coverage_gaps: Vec<CoverageGap>,
    pub summary: OrchestratorSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorSummary {
    pub total_bots: usize,
    pub activate_count: usize,
    pub monitor_count: usize,
    pub pause_count: usize,
    pub mismatches: usize,
    pub best_strategy: Option<String>,
    pub regime_message: String,
}

/// The orchestrator engine
pub struct Orchestrator {
    pool: PgPool,
}

impl Orchestrator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Score a strategy profile against current market conditions
    /// Now uses per-instrument data when an instrument is known
    pub fn score_profile(
        &self, 
        profile: &StrategyProfile, 
        conditions: &MarketConditions, 
        strategy_type: &str,
        instrument: Option<&str>,
    ) -> (f64, Vec<String>) {
        let mut score: f64 = 0.0;
        let mut reasons = Vec::new();
        
        // Look up per-instrument data if instrument is known
        let inst_score: Option<&InstrumentScore> = instrument
            .and_then(|sym| conditions.instrument_scores.get_score(sym));
        
        // Adaptive strategies get bonus for following MRATE
        if profile.adaptive {
            score += 50.0;
            reasons.push("Adaptive strategy (follows MRATE)".to_string());
        }
        
        // ═══════════════════════════════════════════════════════════════
        // PER-INSTRUMENT SCORING (replaces global regime match)
        // ═══════════════════════════════════════════════════════════════
        if let Some(inst) = inst_score {
            let category = strategy_type_to_category(strategy_type);
            
            // Instrument strategy alignment: +40 points
            // Does this strategy type match what the instrument's technicals suggest?
            if let Some(cat) = category {
                if inst.best_strategies.contains(&cat) {
                    score += 40.0;
                    reasons.push(format!(
                        "✅ {} {:?} aligns with {} best strategies", 
                        inst.symbol, cat, inst.price_regime.as_str()
                    ));
                } else {
                    // Partial credit if price regime at least allows this type
                    let allowed = match cat {
                        StrategyCategory::Trend | StrategyCategory::Breakout => inst.price_regime.allows_trend_strategies(),
                        StrategyCategory::MeanReversion | StrategyCategory::LiquiditySweep => inst.price_regime.allows_mean_reversion(),
                    };
                    if allowed {
                        score += 20.0;
                        reasons.push(format!(
                            "⚡ {:?} partially fits {} ({})", 
                            cat, inst.symbol, inst.price_regime.as_str()
                        ));
                    } else {
                        reasons.push(format!(
                            "❌ {:?} misaligned with {} ({})", 
                            cat, inst.symbol, inst.price_regime.as_str()
                        ));
                    }
                }
            } else {
                // Adaptive/unknown category - score based on instrument macro score
                let inst_bonus = (inst.score / 100.0) * 30.0; // 0-30 based on instrument favorability
                score += inst_bonus;
                reasons.push(format!("{} macro score: {:.0}", inst.symbol, inst.score));
            }
            
            // Trading direction bonus: +15 points
            // Stronger bonus when direction is clear (Long/Short vs Neutral)
            match inst.trading_direction {
                TradingDirection::Long => {
                    score += 15.0;
                    reasons.push(format!("📈 {} direction: LONG", inst.symbol));
                }
                TradingDirection::Short => {
                    score += 15.0;
                    reasons.push(format!("📉 {} direction: SHORT", inst.symbol));
                }
                TradingDirection::Neutral => {
                    score += 5.0;
                    reasons.push(format!("↔️ {} direction: NEUTRAL", inst.symbol));
                }
            }
            
            // Momentum/technicals bonus: up to +10 points
            let momentum = inst.technicals.momentum_score;
            // Strong momentum in either direction is good (bidirectional)
            let momentum_strength = (momentum - 50.0).abs(); // 0-50 range
            let momentum_bonus = (momentum_strength / 50.0) * 10.0;
            score += momentum_bonus;
            if momentum_strength > 25.0 {
                let dir_str = if momentum > 50.0 { "bullish" } else { "bearish" };
                reasons.push(format!("Strong {} momentum on {} ({:.0})", dir_str, inst.symbol, momentum));
            }
        } else {
            // Fallback: no instrument known, use legacy global regime scoring
            let regime_str = conditions.regime.as_str();
            if profile.preferred_regimes.iter().any(|r| r == regime_str) {
                score += 40.0;
                reasons.push(format!("Regime match: {} (no instrument)", regime_str));
            } else {
                reasons.push(format!("Regime mismatch: {} not in {:?}", regime_str, profile.preferred_regimes));
            }
            
            // Legacy strategy category weight bonus
            if let Some(category) = strategy_type_to_category(strategy_type) {
                let weight = conditions.strategy_weights.get(category);
                let weight_bonus = weight * 15.0;
                score += weight_bonus;
                if weight >= 0.7 {
                    reasons.push(format!("Strong {:?} weight ({:.0}%)", category, weight * 100.0));
                }
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // SESSION, HOURS, VOLATILITY (unchanged - these are global)
        // ═══════════════════════════════════════════════════════════════
        
        // Session match: +25 points
        if profile.best_sessions.iter().any(|s| s == &conditions.session) || profile.best_sessions.contains(&"ANY".to_string()) {
            score += 25.0;
            reasons.push(format!("Session match: {}", conditions.session));
        } else if conditions.session == "OVERLAP" 
            && (profile.best_sessions.contains(&"LONDON".to_string()) 
                || profile.best_sessions.contains(&"NEW_YORK".to_string())) 
        {
            score += 20.0;
            reasons.push("Overlap session (partial match)".to_string());
        }
        
        // Hours match: +10 points
        let hour = conditions.hour_utc;
        if hour >= profile.hours_utc.start && hour < profile.hours_utc.end {
            score += 10.0;
            reasons.push(format!("Within optimal hours ({}-{})", profile.hours_utc.start, profile.hours_utc.end));
        }
        
        // Volatility match: +10 points
        if profile.volatility_pref == "any" || profile.volatility_pref == conditions.volatility_level {
            score += 10.0;
            reasons.push(format!("Volatility match: {}", conditions.volatility_level));
        } else if (profile.volatility_pref == "high" || profile.volatility_pref == "low") 
            && conditions.volatility_level == "normal" 
        {
            score += 5.0;
            reasons.push(format!("Volatility acceptable (prefers {})", profile.volatility_pref));
        }
        
        // Cap at 100
        score = score.min(100.0);
        
        (score, reasons)
    }
    
    /// Get recommendations for all bots
    pub async fn get_recommendations(&self, mrate: &MrateOutput) -> Result<OrchestratorResponse, sqlx::Error> {
        let conditions = MarketConditions::from_mrate(mrate);
        
        // Fetch all bots with their strategy profiles AND instrument
        let bots = sqlx::query_as::<_, BotWithProfile>(
            r#"
            SELECT 
                b.id as bot_id,
                b.name as bot_name,
                b.is_active as is_enabled,
                COALESCE(b.auto_disabled, false) as auto_disabled,
                s.strategy_type,
                s.profile,
                COALESCE(s.params->>'instrument', s.params->>'symbol') as instrument
            FROM bots b
            JOIN strategies s ON b.strategy_id = s.id
            ORDER BY b.name
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut recommendations = Vec::new();
        let mut best_score = 0.0;
        let mut best_strategy = None;
        
        for bot in bots {
            // Use stored profile if available, otherwise auto-generate based on strategy type
            let profile: StrategyProfile = match &bot.profile {
                Some(p) => serde_json::from_value(p.clone()).unwrap_or_else(|_| {
                    tracing::debug!("Failed to parse profile for {}, using auto-generated", bot.bot_name);
                    generate_profile_for_strategy(&bot.strategy_type)
                }),
                None => generate_profile_for_strategy(&bot.strategy_type),
            };
            
            // Score against the bot's specific instrument data
            let instrument = bot.instrument.as_deref();
            let (mut score, mut reasons) = self.score_profile(&profile, &conditions, &bot.strategy_type, instrument);
            
            // Apply historical performance adjustment from learning module
            if let Some((win_rate, trade_count, avg_pnl)) = query_historical_performance(
                &self.pool, bot.bot_id, conditions.regime.as_str(), &conditions.session
            ).await {
                let (adj, reason) = performance_score_adjustment(win_rate, trade_count, avg_pnl);
                score = (score + adj).clamp(0.0, 100.0);
                reasons.push(reason);
            }
            
            let action = RecommendedAction::from_score(score);
            
            let should_be_enabled = matches!(action, RecommendedAction::Activate | RecommendedAction::Monitor);
            let mismatch = bot.is_enabled != should_be_enabled;
            
            if score > best_score {
                best_score = score;
                best_strategy = Some(bot.strategy_type.clone());
            }
            
            // Auto-disable bots that are running but should be paused
            // ONLY auto-disable if the bot was previously auto-managed (auto_disabled has been
            // set before) — never disable a bot the user manually enabled
            if bot.is_enabled && action == RecommendedAction::Pause && bot.auto_disabled {
                // Bot was previously auto-enabled by orchestrator, safe to auto-disable
                if let Err(e) = self.auto_disable_bot(bot.bot_id, &bot.bot_name, score).await {
                    tracing::error!("Failed to auto-disable bot {}: {}", bot.bot_name, e);
                }
            } else if bot.is_enabled && action == RecommendedAction::Pause && !bot.auto_disabled {
                tracing::info!(
                    "⚡ Bot '{}' scores {:.0} (PAUSE) but was user-enabled — skipping auto-disable",
                    bot.bot_name, score
                );
            }
            
            // Auto-re-enable bots that were auto-disabled but now score well
            if !bot.is_enabled && bot.auto_disabled && action == RecommendedAction::Activate {
                if let Err(e) = self.auto_enable_bot(bot.bot_id, &bot.bot_name, score).await {
                    tracing::error!("Failed to auto-enable bot {}: {}", bot.bot_name, e);
                }
            }
            
            recommendations.push(BotRecommendation {
                bot_id: bot.bot_id,
                bot_name: bot.bot_name,
                strategy_type: bot.strategy_type,
                score,
                action,
                reasons,
                is_currently_enabled: bot.is_enabled,
                mismatch,
            });
        }
        
        // Sort by score descending
        recommendations.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        
        // Check for coverage gaps
        let coverage_gaps = self.detect_coverage_gaps(&recommendations, &conditions).await?;
        
        // Build summary
        let total_bots = recommendations.len();
        let activate_count = recommendations.iter().filter(|r| r.action == RecommendedAction::Activate).count();
        let monitor_count = recommendations.iter().filter(|r| r.action == RecommendedAction::Monitor).count();
        let pause_count = recommendations.iter().filter(|r| r.action == RecommendedAction::Pause).count();
        let mismatches = recommendations.iter().filter(|r| r.mismatch).count();
        
        let regime_message = format!(
            "{} regime in {} session. {}",
            conditions.regime.as_str(),
            conditions.session,
            conditions.regime.description()
        );
        
        Ok(OrchestratorResponse {
            timestamp: Utc::now(),
            conditions,
            recommendations,
            coverage_gaps,
            summary: OrchestratorSummary {
                total_bots,
                activate_count,
                monitor_count,
                pause_count,
                mismatches,
                best_strategy,
                regime_message,
            },
        })
    }
    
    /// Detect gaps where no strategy scores well
    async fn detect_coverage_gaps(
        &self,
        recommendations: &[BotRecommendation],
        conditions: &MarketConditions,
    ) -> Result<Vec<CoverageGap>, sqlx::Error> {
        let mut gaps = Vec::new();
        
        // If best score is below threshold, this is a gap
        let best_score = recommendations.iter().map(|r| r.score).fold(0.0_f64, f64::max);
        
        if best_score < 50.0 {
            let suggestion = match conditions.regime {
                Regime::Panic => "Consider a dedicated safe-haven or inverse strategy".to_string(),
                Regime::Choppy if conditions.volatility_level == "high" => 
                    "High volatility choppy market - consider a volatility arbitrage strategy".to_string(),
                Regime::Trend if conditions.session == "ASIAN" =>
                    "Consider an Asian session trend strategy for overnight moves".to_string(),
                _ => format!(
                    "No ideal strategy for {} regime during {} session", 
                    conditions.regime.as_str(), 
                    conditions.session
                ),
            };
            
            let gap = CoverageGap {
                regime: conditions.regime.as_str().to_string(),
                session: conditions.session.clone(),
                hour_utc: conditions.hour_utc,
                volatility_level: conditions.volatility_level.clone(),
                best_score,
                suggestion,
            };
            
            gaps.push(gap.clone());
            
            // Record the gap in the database for analysis
            self.record_coverage_gap(&gap).await?;
        }
        
        Ok(gaps)
    }
    
    /// Auto-disable a bot that shouldn't be running
    async fn auto_disable_bot(&self, bot_id: Uuid, bot_name: &str, score: f64) -> Result<(), sqlx::Error> {
        tracing::warn!(
            "🛑 Auto-disabling bot '{}' - score {:.0} below threshold (conditions unfavorable)",
            bot_name, score
        );
        
        sqlx::query(
            "UPDATE bots SET is_active = false, auto_disabled = true, updated_at = NOW() WHERE id = $1"
        )
            .bind(bot_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// Auto-re-enable a bot that was previously auto-disabled and now scores well
    async fn auto_enable_bot(&self, bot_id: Uuid, bot_name: &str, score: f64) -> Result<(), sqlx::Error> {
        tracing::info!(
            "✅ Auto-re-enabling bot '{}' - score {:.0} above activation threshold",
            bot_name, score
        );
        
        sqlx::query(
            "UPDATE bots SET is_active = true, auto_disabled = false, updated_at = NOW() WHERE id = $1 AND auto_disabled = true"
        )
            .bind(bot_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// Record a coverage gap for pattern analysis
    async fn record_coverage_gap(&self, gap: &CoverageGap) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO market_coverage_gaps (regime, session, hour_utc, volatility_level, best_strategy_score, last_seen)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (regime, session, hour_utc)
            DO UPDATE SET 
                occurrence_count = market_coverage_gaps.occurrence_count + 1,
                best_strategy_score = LEAST(market_coverage_gaps.best_strategy_score, $5),
                last_seen = NOW()
            "#
        )
        .bind(&gap.regime)
        .bind(&gap.session)
        .bind(gap.hour_utc as i32)
        .bind(&gap.volatility_level)
        .bind(gap.best_score)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Get frequent coverage gaps for strategy development insights
    pub async fn get_frequent_gaps(&self, min_occurrences: i32) -> Result<Vec<FrequentGap>, sqlx::Error> {
        let gaps = sqlx::query_as::<_, FrequentGap>(
            r#"
            SELECT 
                regime,
                session,
                hour_utc,
                volatility_level,
                occurrence_count,
                best_strategy_score,
                first_seen,
                last_seen
            FROM market_coverage_gaps
            WHERE occurrence_count >= $1
            ORDER BY occurrence_count DESC
            LIMIT 10
            "#
        )
        .bind(min_occurrences)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(gaps)
    }
}

/// Database row for bot with profile
#[derive(Debug, sqlx::FromRow)]
struct BotWithProfile {
    bot_id: Uuid,
    bot_name: String,
    is_enabled: bool,
    auto_disabled: bool,
    strategy_type: String,
    profile: Option<serde_json::Value>,
    /// Instrument this bot trades (from strategy params)
    instrument: Option<String>,
}

/// Auto-generate a strategy profile based on strategy type
pub fn generate_profile_for_strategy(strategy_type: &str) -> StrategyProfile {
    match strategy_type {
        // ═══════════════════════════════════════════════════════════════
        // MRATE-DRIVEN STRATEGIES (Adaptive = true → +50 bonus)
        // ═══════════════════════════════════════════════════════════════
        "mrate_regime_trader" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "CHOPPY".to_string(), "GOLD_SUPER_BULL".to_string(), "BTC_SUPER_BULL".to_string(), "PANIC".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string(), "OVERLAP".to_string(), "ASIAN".to_string()],
            hours_utc: HoursRange { start: 0, end: 24 },
            volatility_pref: "any".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 25.0, max: 75.0 },
            min_mrate_weight: 0.6,
            adaptive: true, // Follows MRATE regime
        },
        
        "real_yield_momentum" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "GOLD_SUPER_BULL".to_string(), "PANIC".to_string()],
            best_sessions: vec!["ANY".to_string()], // Macro doesn't care about sessions
            hours_utc: HoursRange { start: 0, end: 24 },
            volatility_pref: "any".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.6,
            adaptive: true, // Uses MRATE risk score
        },
        
        "macro_aligned_momentum" => StrategyProfile {
            preferred_regimes: vec!["GOLD_SUPER_BULL".to_string(), "BTC_SUPER_BULL".to_string(), "TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 30.0, max: 70.0 },
            min_mrate_weight: 0.5,
            adaptive: true, // Uses Polymarket macro data
        },
        
        "gold_dxy_divergence" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 30.0, max: 70.0 },
            min_mrate_weight: 0.5,
            adaptive: true, // Uses MRATE DXY data
        },
        
        "fomc_volatility" => StrategyProfile {
            preferred_regimes: vec!["GOLD_SUPER_BULL".to_string(), "TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "high".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.6,
            adaptive: true, // Uses MRATE calendar + uncertainty
        },
        
        // ═══════════════════════════════════════════════════════════════
        // TREND FOLLOWING STRATEGIES
        // ═══════════════════════════════════════════════════════════════
        "quant_gold_momentum" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "GOLD_SUPER_BULL".to_string(), "PANIC".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string(), "OVERLAP".to_string(), "ASIAN".to_string()],
            hours_utc: HoursRange { start: 7, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 30.0, max: 70.0 },
            min_mrate_weight: 0.4,
            adaptive: false,
        },
        
        "london_ny_trend_continuation" | "london_ny_trend_2" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string(), "OVERLAP".to_string()],
            hours_utc: HoursRange { start: 7, end: 16 }, // London/NY hours
            volatility_pref: "normal".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 35.0, max: 65.0 },
            min_mrate_weight: 0.5,
            adaptive: false,
        },
        
        "sma_crossover" | "ma_crossover" | "ema_crossover" | "macd_crossover" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 18.0,
            rsi_range: RsiRange { min: 30.0, max: 70.0 },
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        // ═══════════════════════════════════════════════════════════════
        // BREAKOUT STRATEGIES
        // ═══════════════════════════════════════════════════════════════
        "donchian_breakout" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "GOLD_SUPER_BULL".to_string(), "BTC_SUPER_BULL".to_string()],
            best_sessions: vec!["LONDON".to_string(), "OVERLAP".to_string()],
            hours_utc: HoursRange { start: 7, end: 12 },
            volatility_pref: "normal".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.4,
            adaptive: false,
        },
        
        "london_breakout" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "GOLD_SUPER_BULL".to_string(), "BTC_SUPER_BULL".to_string()],
            best_sessions: vec!["LONDON".to_string()],
            hours_utc: HoursRange { start: 7, end: 9 }, // London open
            volatility_pref: "high".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        "volatility_expansion" => StrategyProfile {
            preferred_regimes: vec!["GOLD_SUPER_BULL".to_string(), "BTC_SUPER_BULL".to_string(), "TREND".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 0, end: 24 }, // Daily timeframe
            volatility_pref: "high".to_string(),
            min_adx: 15.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.5,
            adaptive: false,
        },
        
        // ═══════════════════════════════════════════════════════════════
        // MEAN REVERSION STRATEGIES
        // ═══════════════════════════════════════════════════════════════
        "rsi_reversion_2" | "rsi_mean_reversion" | "rsi_reversal" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "TREND".to_string(), "PANIC".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string(), "ASIAN".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "low".to_string(),
            min_adx: 0.0, // No ADX requirement for mean reversion
            rsi_range: RsiRange { min: 0.0, max: 30.0 }, // Oversold or overbought
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        "bollinger_mean_reversion" | "bollinger_bounce" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "PANIC".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string(), "ASIAN".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "low".to_string(),
            min_adx: 0.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        "stochastic_crossover" | "stochastic_reversal" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "PANIC".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 8, end: 20 },
            volatility_pref: "low".to_string(),
            min_adx: 0.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        "forecast_confidence" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "TREND".to_string()],
            best_sessions: vec!["ANY".to_string()],
            hours_utc: HoursRange { start: 0, end: 24 }, // Daily forecast
            volatility_pref: "low".to_string(),
            min_adx: 0.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.4,
            adaptive: false,
        },
        
        // ═══════════════════════════════════════════════════════════════
        // LIQUIDITY SWEEP / STRUCTURE STRATEGIES
        // ═══════════════════════════════════════════════════════════════
        "liquidity_sweep" => StrategyProfile {
            preferred_regimes: vec!["CHOPPY".to_string(), "PANIC".to_string(), "TREND".to_string()],
            best_sessions: vec!["NEW_YORK".to_string(), "ASIAN".to_string(), "LONDON".to_string()],
            hours_utc: HoursRange { start: 0, end: 24 },
            volatility_pref: "high".to_string(),
            min_adx: 0.0,
            rsi_range: RsiRange { min: 0.0, max: 30.0 }, // Extremes
            min_mrate_weight: 0.3,
            adaptive: false,
        },
        
        "trendline_bounce" => StrategyProfile {
            preferred_regimes: vec!["TREND".to_string(), "CHOPPY".to_string()],
            best_sessions: vec!["LONDON".to_string(), "NEW_YORK".to_string()],
            hours_utc: HoursRange { start: 7, end: 20 },
            volatility_pref: "normal".to_string(),
            min_adx: 20.0,
            rsi_range: RsiRange { min: 0.0, max: 100.0 },
            min_mrate_weight: 0.4,
            adaptive: false,
        },
        
        // ═══════════════════════════════════════════════════════════════
        // DEFAULT FOR UNKNOWN/CUSTOM STRATEGIES
        // ═══════════════════════════════════════════════════════════════
        _ => StrategyProfile::default(),
    }
}

/// Map strategy_type string to a StrategyCategory for weight lookup
fn strategy_type_to_category(strategy_type: &str) -> Option<StrategyCategory> {
    match strategy_type {
        // Trend following
        "quant_gold_momentum" | "london_ny_trend_continuation" | "london_ny_trend_2"
        | "sma_crossover" | "ma_crossover" | "ema_crossover" | "macd_crossover"
        | "macd_divergence" | "ema_ribbon" | "adx_trend" | "triple_screen"
        | "trendline_bounce" | "real_yield_momentum" | "gold_dxy_divergence" => {
            Some(StrategyCategory::Trend)
        }
        // Breakout
        "donchian_breakout" | "london_breakout" | "atr_breakout" | "volatility_expansion"
        | "asian_range" => {
            Some(StrategyCategory::Breakout)
        }
        // Mean reversion
        "rsi_reversion_2" | "rsi_mean_reversion" | "rsi_reversal" | "bollinger_mean_reversion"
        | "bollinger_bounce" | "stochastic_crossover" | "stochastic_reversal"
        | "forecast_confidence" => {
            Some(StrategyCategory::MeanReversion)
        }
        // Liquidity sweep / structure
        "liquidity_sweep" => Some(StrategyCategory::LiquiditySweep),
        // Adaptive / macro (these already get +50 adaptive bonus, no category needed)
        "mrate_regime_trader" | "macro_aligned_momentum" | "fomc_volatility" => None,
        // Unknown
        _ => None,
    }
}

/// Frequent gap for analysis
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FrequentGap {
    pub regime: String,
    pub session: String,
    pub hour_utc: i32,
    pub volatility_level: Option<String>,
    pub occurrence_count: i32,
    pub best_strategy_score: Option<f64>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// AI BRAIN RECOMMENDATIONS - Operational Intelligence
// ═══════════════════════════════════════════════════════════════════════════════

/// AI Brain recommendation for operational improvements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIRecommendation {
    pub priority: String,       // "critical", "high", "medium", "low"
    pub category: String,       // "timeframe_gap", "strategy_gap", "risk", "opportunity"
    pub title: String,
    pub description: String,
    pub action: Option<String>, // Suggested action
    pub icon: String,
}

/// Timeframe coverage summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeframeCoverage {
    pub timeframe: String,
    pub active_bots: i32,
    pub signals_per_day_estimate: f64,
    pub is_covered: bool,
}

/// Full AI Brain analysis response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIBrainAnalysis {
    pub timestamp: DateTime<Utc>,
    pub regime: String,
    pub regime_description: String,
    pub otp_score: f64,           // Optimal Trade Probability
    pub timeframe_coverage: Vec<TimeframeCoverage>,
    pub recommendations: Vec<AIRecommendation>,
    pub active_bot_count: i32,
    pub healthy: bool,            // Overall system health
}

impl Orchestrator {
    /// Analyze system and generate AI Brain recommendations
    pub async fn analyze_for_ai_brain(&self, mrate: &MrateOutput) -> Result<AIBrainAnalysis, sqlx::Error> {
        let mut recommendations = Vec::new();
        let conditions = MarketConditions::from_mrate(mrate);
        
        // Get active bots with timeframes and MRATE categories
        let bots = sqlx::query!(
            r#"
            SELECT 
                b.id,
                b.name,
                b.is_active,
                s.strategy_type,
                s.params->>'timeframe' as timeframe,
                COALESCE(s.params->>'instrument', s.params->>'symbol') as instrument,
                s.mrate_category
            FROM bots b
            JOIN strategies s ON b.strategy_id = s.id
            WHERE b.is_active = true
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let active_bot_count = bots.len() as i32;
        
        // Analyze timeframe distribution
        let mut tf_counts: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
        for bot in &bots {
            let tf = bot.timeframe.clone().unwrap_or_else(|| "D".to_string());
            *tf_counts.entry(tf).or_insert(0) += 1;
        }
        
        // Build timeframe coverage with signal frequency estimates
        let timeframe_coverage = vec![
            TimeframeCoverage {
                timeframe: "M15".to_string(),
                active_bots: *tf_counts.get("M15").unwrap_or(&0),
                signals_per_day_estimate: (*tf_counts.get("M15").unwrap_or(&0) as f64) * 8.0, // ~8 signals/day per M15 bot
                is_covered: *tf_counts.get("M15").unwrap_or(&0) > 0,
            },
            TimeframeCoverage {
                timeframe: "M30".to_string(),
                active_bots: *tf_counts.get("M30").unwrap_or(&0),
                signals_per_day_estimate: (*tf_counts.get("M30").unwrap_or(&0) as f64) * 4.0,
                is_covered: *tf_counts.get("M30").unwrap_or(&0) > 0,
            },
            TimeframeCoverage {
                timeframe: "H1".to_string(),
                active_bots: *tf_counts.get("H1").unwrap_or(&0),
                signals_per_day_estimate: (*tf_counts.get("H1").unwrap_or(&0) as f64) * 2.0,
                is_covered: *tf_counts.get("H1").unwrap_or(&0) > 0,
            },
            TimeframeCoverage {
                timeframe: "H4".to_string(),
                active_bots: *tf_counts.get("H4").unwrap_or(&0),
                signals_per_day_estimate: (*tf_counts.get("H4").unwrap_or(&0) as f64) * 0.5,
                is_covered: *tf_counts.get("H4").unwrap_or(&0) > 0,
            },
            TimeframeCoverage {
                timeframe: "D".to_string(),
                active_bots: *tf_counts.get("D").unwrap_or(&0),
                signals_per_day_estimate: (*tf_counts.get("D").unwrap_or(&0) as f64) * 0.1, // ~1 signal per 10 days
                is_covered: *tf_counts.get("D").unwrap_or(&0) > 0,
            },
        ];
        
        // Calculate total expected signals per day
        let total_signals_per_day: f64 = timeframe_coverage.iter().map(|tc| tc.signals_per_day_estimate).sum();
        
        // CRITICAL: No intraday coverage
        let has_intraday = *tf_counts.get("M15").unwrap_or(&0) > 0 
            || *tf_counts.get("M30").unwrap_or(&0) > 0
            || *tf_counts.get("H1").unwrap_or(&0) > 0;
        
        let daily_only = *tf_counts.get("D").unwrap_or(&0) > 0 && !has_intraday && *tf_counts.get("H4").unwrap_or(&0) == 0;
        
        if !has_intraday && active_bot_count > 0 {
            let severity = if daily_only { "critical" } else { "high" };
            recommendations.push(AIRecommendation {
                priority: severity.to_string(),
                category: "timeframe_gap".to_string(),
                title: "Missing Intraday Coverage".to_string(),
                description: format!(
                    "You have {} active bots but none on M15/M30/H1 timeframes. \
                    Daily bots may only signal 1-4 times per month. \
                    Expected signals/day: {:.1}",
                    active_bot_count, total_signals_per_day
                ),
                action: Some("Add H1 or M15 versions of your best-performing strategies".to_string()),
                icon: "⚠️".to_string(),
            });
        }
        
        // Check if regime matches active strategy types (global)
        let mut has_mean_reversion = false;
        let mut has_trend = false;
        let mut has_breakout = false;
        
        // Build per-instrument bot mapping: instrument -> [(bot_name, strategy_type, category)]
        let mut instrument_bots: std::collections::HashMap<String, Vec<(String, String, Option<StrategyCategory>)>> = 
            std::collections::HashMap::new();
        
        for bot in &bots {
            let st = bot.strategy_type.as_str();
            if st.contains("bollinger") || st.contains("rsi") || st.contains("stochastic") || st.contains("mean_reversion") {
                has_mean_reversion = true;
            }
            if st.contains("trend") || st.contains("macd") || st.contains("sma") || st.contains("ema") || st.contains("momentum") {
                has_trend = true;
            }
            if st.contains("breakout") || st.contains("donchian") || st.contains("london") {
                has_breakout = true;
            }
            
            // Map bot to its instrument
            if let Some(ref inst) = bot.instrument {
                let category = bot.mrate_category.as_deref()
                    .and_then(StrategyCategory::from_str)
                    .or_else(|| strategy_type_to_category(st));
                instrument_bots.entry(inst.clone())
                    .or_default()
                    .push((bot.name.clone(), st.to_string(), category));
            }
        }
        
        // ═══════════════════════════════════════════════════════════════
        // PER-INSTRUMENT BOT COVERAGE ANALYSIS
        // Compare what's deployed vs what conditions demand
        // ═══════════════════════════════════════════════════════════════
        let instrument_conditions = [
            ("XAU_USD", &conditions.instrument_scores.gold, "Gold"),
            ("BTC_USD", &conditions.instrument_scores.bitcoin, "Bitcoin"),
            ("EUR_USD", &conditions.instrument_scores.eur_usd, "EUR/USD"),
            ("USD_JPY", &conditions.instrument_scores.usd_jpy, "USD/JPY"),
            ("WTICO_USD", &conditions.instrument_scores.wti_oil, "WTI Oil"),
            ("NATGAS_USD", &conditions.instrument_scores.natural_gas, "Natural Gas"),
        ];
        
        let mut strong_longs: Vec<String> = Vec::new();
        let mut strong_shorts: Vec<String> = Vec::new();
        let mut trending_instruments: Vec<String> = Vec::new();
        
        for (symbol, inst, display_name) in &instrument_conditions {
            let dir_str = match inst.trading_direction {
                TradingDirection::Long => { strong_longs.push(display_name.to_string()); "LONG" },
                TradingDirection::Short => { strong_shorts.push(display_name.to_string()); "SHORT" },
                TradingDirection::Neutral => "NEUTRAL",
            };
            if inst.price_regime == PriceRegime::Trending {
                trending_instruments.push(display_name.to_string());
            }
            
            let deployed_bots = instrument_bots.get(*symbol);
            let bot_count = deployed_bots.map(|b| b.len()).unwrap_or(0);
            
            // What strategy types does this instrument need?
            let needs_trend = inst.price_regime == PriceRegime::Trending || inst.price_regime == PriceRegime::Transitional;
            let needs_mr = inst.price_regime == PriceRegime::Ranging || inst.price_regime == PriceRegime::Transitional;
            
            // What's actually deployed?
            let has_trend_bot = deployed_bots.map(|bs| bs.iter().any(|(_, _, cat)| {
                matches!(cat, Some(StrategyCategory::Trend) | Some(StrategyCategory::Breakout))
            })).unwrap_or(false);
            let has_mr_bot = deployed_bots.map(|bs| bs.iter().any(|(_, _, cat)| {
                matches!(cat, Some(StrategyCategory::MeanReversion) | Some(StrategyCategory::LiquiditySweep))
            })).unwrap_or(false);
            let has_no_category = deployed_bots.map(|bs| bs.iter().any(|(_, _, cat)| cat.is_none())).unwrap_or(false);
            
            // NO BOTS AT ALL on this instrument
            if bot_count == 0 && inst.trading_direction != TradingDirection::Neutral {
                let strategy_suggestion = match inst.price_regime {
                    PriceRegime::Trending => "trend-following (MACD, EMA crossover, momentum)",
                    PriceRegime::Ranging => "mean reversion (Stochastic, Bollinger, RSI)",
                    PriceRegime::Transitional => "trend or mean reversion",
                };
                recommendations.push(AIRecommendation {
                    priority: "high".to_string(),
                    category: "instrument_gap".to_string(),
                    title: format!("{} — No Bots Deployed", display_name),
                    description: format!(
                        "{} is {} (ADX={:.0}) with {} direction but has no active bots. \
                        Conditions favour {} strategies.",
                        display_name, inst.price_regime.as_str(), inst.trend_strength,
                        dir_str, strategy_suggestion
                    ),
                    action: Some(format!("Deploy a {} bot on {} targeting {} entries",
                        strategy_suggestion, symbol, dir_str)),
                    icon: "🚨".to_string(),
                });
                continue;
            }
            
            // WRONG STRATEGY TYPE for conditions
            if needs_mr && !has_mr_bot && bot_count > 0 && inst.price_regime == PriceRegime::Ranging {
                let bot_names: Vec<_> = deployed_bots.unwrap().iter().map(|(n, _, _)| n.as_str()).collect();
                recommendations.push(AIRecommendation {
                    priority: "medium".to_string(),
                    category: "strategy_mismatch".to_string(),
                    title: format!("{} Ranging — No Mean Reversion Bots", display_name),
                    description: format!(
                        "{} is RANGING (ADX={:.0}) — mean reversion strategies are optimal but deployed bots ({}) \
                        are trend/breakout type. They will trade with reduced weight.",
                        display_name, inst.trend_strength, bot_names.join(", ")
                    ),
                    action: Some(format!("Add a Stochastic, Bollinger Bounce, or RSI Reversion bot on {}", symbol)),
                    icon: "🔄".to_string(),
                });
            }
            
            if needs_trend && !has_trend_bot && bot_count > 0 && inst.price_regime == PriceRegime::Trending {
                let bot_names: Vec<_> = deployed_bots.unwrap().iter().map(|(n, _, _)| n.as_str()).collect();
                recommendations.push(AIRecommendation {
                    priority: "medium".to_string(),
                    category: "strategy_mismatch".to_string(),
                    title: format!("{} Trending — No Trend Bots", display_name),
                    description: format!(
                        "{} is TRENDING {} (ADX={:.0}) but deployed bots ({}) are mean reversion type. \
                        They will trade with reduced weight against the trend.",
                        display_name, dir_str, inst.trend_strength, bot_names.join(", ")
                    ),
                    action: Some(format!("Add a MACD, EMA crossover, or momentum bot on {} targeting {} entries",
                        symbol, dir_str)),
                    icon: "📈".to_string(),
                });
            }
            
            // BOTS WITHOUT MRATE CATEGORY — bypassing MRATE filtering entirely
            if has_no_category {
                let uncategorized: Vec<_> = deployed_bots.unwrap().iter()
                    .filter(|(_, _, cat)| cat.is_none())
                    .map(|(n, _, _)| n.as_str())
                    .collect();
                recommendations.push(AIRecommendation {
                    priority: "low".to_string(),
                    category: "configuration".to_string(),
                    title: format!("{} — Bots Missing MRATE Category", display_name),
                    description: format!(
                        "Bots ({}) on {} have no MRATE category set — they bypass all MRATE \
                        regime/direction filtering. Set mrate_category in the strategy to enable intelligent gating.",
                        uncategorized.join(", "), display_name
                    ),
                    action: Some("Edit strategy settings to assign an MRATE category".to_string()),
                    icon: "⚙️".to_string(),
                });
            }
        }
        
        // Flag if trending instruments exist but no trend strategies globally
        if !trending_instruments.is_empty() && !has_trend && active_bot_count > 0 {
            recommendations.push(AIRecommendation {
                priority: "high".to_string(),
                category: "strategy_gap".to_string(),
                title: "Trending Instruments - No Trend Strategies Active".to_string(),
                description: format!(
                    "{} are trending but no trend-following bots are enabled.",
                    trending_instruments.join(", ")
                ),
                action: Some("Enable MACD, SMA crossover, or momentum strategies".to_string()),
                icon: "📈".to_string(),
            });
        }
        
        // Directional opportunity summaries
        if !strong_longs.is_empty() {
            recommendations.push(AIRecommendation {
                priority: "low".to_string(),
                category: "opportunity".to_string(),
                title: "Long Opportunities Detected".to_string(),
                description: format!(
                    "Technicals favour longs on: {}. Ensure appropriate bots are active.",
                    strong_longs.join(", ")
                ),
                action: None,
                icon: "📈".to_string(),
            });
        }
        if !strong_shorts.is_empty() {
            recommendations.push(AIRecommendation {
                priority: "low".to_string(),
                category: "opportunity".to_string(),
                title: "Short Opportunities Detected".to_string(),
                description: format!(
                    "Technicals favour shorts on: {}. Ensure appropriate bots are active.",
                    strong_shorts.join(", ")
                ),
                action: None,
                icon: "📉".to_string(),
            });
        }
        
        // Panic regime global warning (still useful as a global observation)
        if conditions.regime == Regime::Panic && conditions.volatility_level == "high" {
            recommendations.push(AIRecommendation {
                priority: "medium".to_string(),
                category: "risk".to_string(),
                title: "Panic Regime - Elevated Risk".to_string(),
                description: "High volatility panic conditions. Consider reducing position sizes.".to_string(),
                action: Some("Review risk settings and consider smaller lot sizes".to_string()),
                icon: "🛡️".to_string(),
            });
        }
        
        // OTP Score (Optimal Trade Probability)
        let otp_score = self.calculate_otp(&conditions, &timeframe_coverage, has_mean_reversion, has_trend);
        
        // Overall health assessment
        let healthy = active_bot_count > 0 
            && has_intraday 
            && recommendations.iter().filter(|r| r.priority == "critical").count() == 0;
        
        // Sort recommendations by priority
        recommendations.sort_by(|a, b| {
            let priority_order = |p: &str| match p {
                "critical" => 0,
                "high" => 1,
                "medium" => 2,
                "low" => 3,
                _ => 4,
            };
            priority_order(&a.priority).cmp(&priority_order(&b.priority))
        });
        
        Ok(AIBrainAnalysis {
            timestamp: Utc::now(),
            regime: conditions.regime.as_str().to_string(),
            regime_description: conditions.regime.description().to_string(),
            otp_score,
            timeframe_coverage,
            recommendations,
            active_bot_count,
            healthy,
        })
    }
    
    /// Calculate Optimal Trade Probability score
    fn calculate_otp(
        &self, 
        conditions: &MarketConditions, 
        timeframe_coverage: &[TimeframeCoverage],
        has_mean_reversion: bool,
        has_trend: bool,
    ) -> f64 {
        let mut score = 50.0; // Base score
        
        // Regime alignment bonus
        match conditions.regime {
            Regime::Choppy if has_mean_reversion => score += 20.0,
            Regime::Trend if has_trend => score += 20.0,
            Regime::GoldSuperBull | Regime::BtcSuperBull => score += 15.0,
            Regime::Panic => score -= 20.0,
            _ => {}
        }
        
        // Session bonus (London/NY overlap is best)
        if conditions.session == "OVERLAP" {
            score += 15.0;
        } else if conditions.session == "LONDON" || conditions.session == "NEW_YORK" {
            score += 10.0;
        } else if conditions.session == "OFF_HOURS" {
            score -= 10.0;
        }
        
        // Timeframe coverage bonus
        let intraday_bots: i32 = timeframe_coverage.iter()
            .filter(|tc| tc.timeframe == "M15" || tc.timeframe == "M30" || tc.timeframe == "H1")
            .map(|tc| tc.active_bots)
            .sum();
        
        if intraday_bots >= 3 {
            score += 15.0;
        } else if intraday_bots > 0 {
            score += 5.0;
        } else {
            score -= 20.0; // Penalty for no intraday
        }
        
        // Liquidity bonus
        score += (conditions.liquidity_score - 50.0) * 0.2;
        
        // Risk penalty
        if conditions.risk_score > 70.0 {
            score -= 15.0;
        }
        
        score.clamp(0.0, 100.0)
    }
}
