//! MRATE Advisor - Self-Improving Feedback System
//!
//! Monitors MRATE decisions vs actual outcomes and generates actionable suggestions.
//! This is the "learning" part of the AI Brain that detects contradictions and recommends
//! parameter changes.

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::models::{
    MrateOutput, Regime, StrategyCategory, StrategyWeights, InstrumentScore, 
    InstrumentRecommendation, PriceRegime, TrendDirection, TradingDirection,
};

/// Types of contradictions the advisor can detect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContradictionType {
    /// Trend strategy blocked but instrument ADX shows strong trend
    TrendBlockedButTrending,
    /// Instrument score < 40 (AVOID) but open short positions are profitable
    AvoidScoreButShortsProfit,
    /// Instrument score < 40 (AVOID) but open long positions are profitable  
    AvoidScoreButLongsProfit,
    /// Global regime says CHOPPY but instrument is clearly trending
    ChoppyRegimeTrendingInstrument,
    /// Regime says favorable but trades are losing
    FavorableRegimeButLosing,
    /// Mean reversion blocked but instrument is ranging (ADX < 20)
    MeanReversionBlockedButRanging,
    /// High ADX but all trend bots blocked
    StrongTrendNoTradingAllowed,
    /// Open position direction opposes instrument's derived trading direction
    PositionAgainstDirection,
    /// Instrument direction changed (was Long, now Short or vice versa)
    DirectionFlipped,
}

impl ContradictionType {
    pub fn severity(&self) -> &'static str {
        match self {
            Self::TrendBlockedButTrending => "high",
            Self::AvoidScoreButShortsProfit => "medium",
            Self::AvoidScoreButLongsProfit => "medium",
            Self::ChoppyRegimeTrendingInstrument => "high",
            Self::FavorableRegimeButLosing => "medium",
            Self::MeanReversionBlockedButRanging => "medium",
            Self::StrongTrendNoTradingAllowed => "high",
            Self::PositionAgainstDirection => "high",
            Self::DirectionFlipped => "high",
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            Self::TrendBlockedButTrending => "📈",
            Self::AvoidScoreButShortsProfit => "🔻",
            Self::AvoidScoreButLongsProfit => "🔺",
            Self::ChoppyRegimeTrendingInstrument => "🔄",
            Self::FavorableRegimeButLosing => "📉",
            Self::MeanReversionBlockedButRanging => "↔️",
            Self::StrongTrendNoTradingAllowed => "🚫",
            Self::PositionAgainstDirection => "⚠️",
            Self::DirectionFlipped => "🔀",
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TrendBlockedButTrending => "TREND_BLOCKED_BUT_TRENDING",
            Self::AvoidScoreButShortsProfit => "AVOID_SCORE_BUT_SHORTS_PROFIT",
            Self::AvoidScoreButLongsProfit => "AVOID_SCORE_BUT_LONGS_PROFIT",
            Self::ChoppyRegimeTrendingInstrument => "CHOPPY_REGIME_TRENDING_INSTRUMENT",
            Self::FavorableRegimeButLosing => "FAVORABLE_REGIME_BUT_LOSING",
            Self::MeanReversionBlockedButRanging => "MEAN_REVERSION_BLOCKED_BUT_RANGING",
            Self::StrongTrendNoTradingAllowed => "STRONG_TREND_NO_TRADING_ALLOWED",
            Self::PositionAgainstDirection => "POSITION_AGAINST_DIRECTION",
            Self::DirectionFlipped => "DIRECTION_FLIPPED",
        }
    }
}

/// A detected contradiction between MRATE state and reality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub contradiction_type: ContradictionType,
    pub instrument: String,
    pub severity: String,
    pub icon: String,
    pub description: String,
    pub evidence: Vec<String>,
    pub detected_at: DateTime<Utc>,
}

/// Suggestion priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionPriority {
    Critical,
    High,
    Medium,
    Low,
}

impl SuggestionPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// Suggestion category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionCategory {
    /// Tune a numeric parameter
    ParameterTune,
    /// Override a blocking decision
    OverrideSuggest,
    /// Adjust regime determination
    RegimeAdjust,
    /// Risk-related alert
    RiskAlert,
    /// Strategy weight adjustment
    WeightAdjust,
}

impl SuggestionCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ParameterTune => "parameter_tune",
            Self::OverrideSuggest => "override_suggest",
            Self::RegimeAdjust => "regime_adjust",
            Self::RiskAlert => "risk_alert",
            Self::WeightAdjust => "weight_adjust",
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            Self::ParameterTune => "🔧",
            Self::OverrideSuggest => "⚡",
            Self::RegimeAdjust => "🔄",
            Self::RiskAlert => "⚠️",
            Self::WeightAdjust => "⚖️",
        }
    }
}

/// An actionable suggestion from the advisor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub priority: SuggestionPriority,
    pub category: SuggestionCategory,
    pub title: String,
    pub description: String,
    pub current_value: Option<String>,
    pub suggested_value: Option<String>,
    pub expected_impact: String,
    pub confidence: f64,
    pub supporting_data: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// A blocked opportunity that could have been profitable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedOpportunity {
    pub instrument: String,
    pub strategy_category: String,
    pub block_reason: String,
    pub potential_direction: String,
    pub adx_value: f64,
    pub price_regime: String,
    pub estimated_miss: Option<f64>,  // Estimated P&L if trade had been taken
}

/// Performance by regime - tracks how trades perform in each regime
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegimePerformance {
    pub regime: String,
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub win_rate: f64,
}

/// Position info from the trade analyzer
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub instrument: String,
    pub direction: String,  // "LONG" or "SHORT"
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub entry_price: f64,
    pub current_price: f64,
}

/// The MRATE Advisor output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorOutput {
    pub timestamp: DateTime<Utc>,
    pub contradictions: Vec<Contradiction>,
    pub suggestions: Vec<Suggestion>,
    pub blocked_opportunities: Vec<BlockedOpportunity>,
    pub performance_by_regime: Vec<RegimePerformance>,
    pub health_score: f64,  // 0-100, how well MRATE is performing
    pub summary: String,
}

/// The MRATE Advisor engine
pub struct MrateAdvisor {
    pool: PgPool,
}

impl MrateAdvisor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Analyze MRATE state and generate insights
    pub async fn analyze(
        &self,
        mrate: &MrateOutput,
        open_positions: &[PositionInfo],
    ) -> AdvisorOutput {
        let now = Utc::now();
        let mut contradictions = Vec::new();
        let mut suggestions = Vec::new();
        let mut blocked_opportunities = Vec::new();
        
        // 1. Check for instrument-level contradictions
        self.check_instrument_contradictions(mrate, open_positions, &mut contradictions, &mut suggestions);
        
        // 2. Check for regime contradictions
        self.check_regime_contradictions(mrate, open_positions, &mut contradictions, &mut suggestions);
        
        // 3. Check strategy weight issues
        self.check_strategy_weight_issues(mrate, &mut suggestions, &mut blocked_opportunities);
        
        // 4. Analyze position performance vs regime
        self.analyze_position_performance(mrate, open_positions, &mut contradictions, &mut suggestions);
        
        // 5. Get historical regime performance
        let performance_by_regime = self.get_regime_performance().await;
        
        // 6. Calculate health score
        let health_score = self.calculate_health_score(&contradictions, &suggestions, &performance_by_regime);
        
        // 7. Generate summary
        let summary = self.generate_summary(&contradictions, &suggestions, health_score);
        
        // Sort suggestions by priority
        suggestions.sort_by(|a, b| a.priority.cmp(&b.priority));
        
        AdvisorOutput {
            timestamp: now,
            contradictions,
            suggestions,
            blocked_opportunities,
            performance_by_regime,
            health_score,
            summary,
        }
    }
    
    /// Check for contradictions at the instrument level
    fn check_instrument_contradictions(
        &self,
        mrate: &MrateOutput,
        positions: &[PositionInfo],
        contradictions: &mut Vec<Contradiction>,
        suggestions: &mut Vec<Suggestion>,
    ) {
        let now = Utc::now();
        let instruments = [
            ("XAU_USD", &mrate.instrument_scores.gold),
            ("BTC_USD", &mrate.instrument_scores.bitcoin),
            ("EUR_USD", &mrate.instrument_scores.eur_usd),
            ("USD_JPY", &mrate.instrument_scores.usd_jpy),
            ("WTICO_USD", &mrate.instrument_scores.wti_oil),
            ("NATGAS_USD", &mrate.instrument_scores.natural_gas),
        ];
        
        for (symbol, score) in instruments {
            // NOTE: Global weight blocking is no longer relevant — per-instrument regime
            // is now the authority in should_trade_instrument(). We only flag genuine
            // per-instrument contradictions here.
            
            // Check: AVOID score but shorts are profitable
            if score.score < 40.0 && score.recommendation == InstrumentRecommendation::Avoid {
                // Look for profitable shorts on this instrument
                let profitable_shorts: Vec<_> = positions.iter()
                    .filter(|p| p.instrument == symbol && p.direction == "SHORT" && p.unrealized_pnl > 0.0)
                    .collect();
                
                if !profitable_shorts.is_empty() {
                    let total_profit: f64 = profitable_shorts.iter().map(|p| p.unrealized_pnl).sum();
                    
                    contradictions.push(Contradiction {
                        contradiction_type: ContradictionType::AvoidScoreButShortsProfit,
                        instrument: symbol.to_string(),
                        severity: "medium".to_string(),
                        icon: "🔻".to_string(),
                        description: format!(
                            "{} score={:.0} (AVOID) but {} short positions are profitable (${:.2})",
                            symbol, score.score, profitable_shorts.len(), total_profit
                        ),
                        evidence: vec![
                            format!("MRATE score: {:.0} (for longs)", score.score),
                            format!("Open shorts: {} profitable", profitable_shorts.len()),
                            format!("Short P&L: ${:.2}", total_profit),
                        ],
                        detected_at: now,
                    });
                    
                    suggestions.push(Suggestion {
                        id: format!("avoid-score-shorts-{}", symbol.to_lowercase().replace("_", "")),
                        priority: SuggestionPriority::Medium,
                        category: SuggestionCategory::RegimeAdjust,
                        title: format!("Low score signals SHORT opportunity for {}", symbol),
                        description: format!(
                            "Score {:.0} indicates AVOID for longs, but this actually signals SHORT opportunities. Current shorts are profitable at ${:.2}.",
                            score.score, total_profit
                        ),
                        current_value: None,
                        suggested_value: None,
                        expected_impact: "Consider interpreting low scores as SHORT signals rather than pure avoidance".to_string(),
                        confidence: 0.75,
                        supporting_data: vec![
                            format!("Shorts profiting: ${:.2}", total_profit),
                            format!("Score represents long favorability, not tradability"),
                        ],
                        created_at: now,
                    });
                }
            }
            
            // Check: Open positions opposing the instrument's derived trading direction
            // Aggregate per instrument (not per position) to avoid duplicate noise
            let dir_str = match score.trading_direction {
                TradingDirection::Long => "LONG",
                TradingDirection::Short => "SHORT",
                TradingDirection::Neutral => continue, // Neutral = no contradiction possible
            };
            
            let opposing_positions: Vec<_> = positions.iter()
                .filter(|p| p.instrument == symbol)
                .filter(|p| match (&score.trading_direction, p.direction.as_str()) {
                    (TradingDirection::Long, "SHORT") => true,
                    (TradingDirection::Short, "LONG") => true,
                    _ => false,
                })
                .collect();
            
            if !opposing_positions.is_empty() {
                let count = opposing_positions.len();
                let total_pnl: f64 = opposing_positions.iter().map(|p| p.unrealized_pnl).sum();
                let pos_direction = &opposing_positions[0].direction;
                let any_losing = opposing_positions.iter().any(|p| p.unrealized_pnl < 0.0);
                
                contradictions.push(Contradiction {
                    contradiction_type: ContradictionType::PositionAgainstDirection,
                    instrument: symbol.to_string(),
                    severity: if any_losing { "high" } else { "medium" }.to_string(),
                    icon: "⚠️".to_string(),
                    description: format!(
                        "{} {} position(s) are {} but technicals say {} (RSI={:.0}, MACD={}, +DI={:.0}/-DI={:.0}) — total P&L: ${:.2}",
                        count, symbol, pos_direction, dir_str,
                        score.technicals.rsi,
                        if score.technicals.macd_histogram > 0.0 { "bullish" } else { "bearish" },
                        score.technicals.plus_di, score.technicals.minus_di,
                        total_pnl
                    ),
                    evidence: vec![
                        format!("{} opposing positions", count),
                        format!("Position direction: {}", pos_direction),
                        format!("MRATE direction: {}", dir_str),
                        format!("RSI: {:.1}", score.technicals.rsi),
                        format!("MACD histogram: {:.4}", score.technicals.macd_histogram),
                        format!("+DI: {:.1}, -DI: {:.1}", score.technicals.plus_di, score.technicals.minus_di),
                        format!("Combined P&L: ${:.2}", total_pnl),
                    ],
                    detected_at: now,
                });
                
                let priority = if any_losing {
                    SuggestionPriority::Critical
                } else {
                    SuggestionPriority::High
                };
                
                suggestions.push(Suggestion {
                    id: format!("pos-vs-dir-{}-{}", symbol.to_lowercase().replace("_", ""), pos_direction.to_lowercase()),
                    priority,
                    category: SuggestionCategory::RiskAlert,
                    title: format!("{}: {} {} position(s) oppose {} direction", symbol, count, pos_direction, dir_str),
                    description: format!(
                        "You have {} {} position(s) on {} but technicals (RSI, MACD, +DI/-DI, EMAs) indicate {}. Combined P&L: ${:.2}. {}.",
                        count, pos_direction, symbol, dir_str, total_pnl,
                        if total_pnl < 0.0 {
                            "Positions are net negative — consider closing"
                        } else {
                            "Positions are net positive — consider taking profit before reversal"
                        }
                    ),
                    current_value: Some(format!("{} x {} positions", count, pos_direction)),
                    suggested_value: Some(format!("Close or flip to {}", dir_str)),
                    expected_impact: "Align positions with per-instrument technical consensus".to_string(),
                    confidence: 0.80,
                    supporting_data: vec![
                        format!("Momentum score: {:.0}", score.technicals.momentum_score),
                        format!("EMA alignment: {}", if score.technicals.ema_50 > score.technicals.ema_200 { "Golden cross" } else { "Death cross" }),
                    ],
                    created_at: now,
                });
            }
        }
    }
    
    /// Check for regime-level observations (informational, not blocking)
    /// Global regime is now observation-only — per-instrument regime is the authority.
    fn check_regime_contradictions(
        &self,
        mrate: &MrateOutput,
        _positions: &[PositionInfo],
        contradictions: &mut Vec<Contradiction>,
        _suggestions: &mut Vec<Suggestion>,
    ) {
        let now = Utc::now();
        
        // Informational: note when global regime diverges from per-instrument regimes
        // This is no longer a "contradiction" that blocks trades — just an observation
        if mrate.regime == Regime::Choppy {
            let instrument_data = [
                ("XAU_USD", &mrate.instrument_scores.gold),
                ("BTC_USD", &mrate.instrument_scores.bitcoin),
                ("EUR_USD", &mrate.instrument_scores.eur_usd),
                ("USD_JPY", &mrate.instrument_scores.usd_jpy),
                ("WTICO_USD", &mrate.instrument_scores.wti_oil),
                ("NATGAS_USD", &mrate.instrument_scores.natural_gas),
            ];
            let trending_instruments: Vec<_> = instrument_data
                .iter()
                .filter(|(_, score)| score.price_regime == PriceRegime::Trending && score.trend_strength >= 25.0)
                .collect();
            
            if !trending_instruments.is_empty() {
                let instruments_str = trending_instruments.iter()
                    .map(|(sym, score)| {
                        let dir = match score.trading_direction {
                            TradingDirection::Long => "LONG",
                            TradingDirection::Short => "SHORT",
                            TradingDirection::Neutral => "NEUTRAL",
                        };
                        format!("{} (ADX={:.0}, {})", sym, score.trend_strength, dir)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                
                contradictions.push(Contradiction {
                    contradiction_type: ContradictionType::ChoppyRegimeTrendingInstrument,
                    instrument: "GLOBAL".to_string(),
                    severity: "low".to_string(), // Demoted — per-instrument regime handles this now
                    icon: "ℹ️".to_string(),
                    description: format!(
                        "Global regime is CHOPPY (observation only) but {} are trending — per-instrument regime allows trading",
                        instruments_str
                    ),
                    evidence: trending_instruments.iter()
                        .map(|(sym, score)| format!("{}: ADX={:.0}, {:?}", sym, score.trend_strength, score.trend_direction))
                        .collect(),
                    detected_at: now,
                });
            }
        }
    }
    
    /// Report per-instrument trading notes
    /// Ranging instruments are NOT blocked — trend bots trade with reduced weight (Rule 2b).
    /// Only flag truly blocked situations (direction actively opposing).
    fn check_strategy_weight_issues(
        &self,
        mrate: &MrateOutput,
        _suggestions: &mut Vec<Suggestion>,
        blocked_opportunities: &mut Vec<BlockedOpportunity>,
    ) {
        let instruments = [
            ("XAU_USD", &mrate.instrument_scores.gold),
            ("BTC_USD", &mrate.instrument_scores.bitcoin),
            ("EUR_USD", &mrate.instrument_scores.eur_usd),
            ("USD_JPY", &mrate.instrument_scores.usd_jpy),
            ("WTICO_USD", &mrate.instrument_scores.wti_oil),
            ("NATGAS_USD", &mrate.instrument_scores.natural_gas),
        ];
        
        for (symbol, score) in instruments {
            // Only flag instruments where direction actively opposes ALL bot types
            // Ranging instruments are NOT blocked — Rule 2b allows trend bots with reduced weight
            // Trending instruments are NOT blocked — Rule 2b allows MR bots with reduced weight
            
            // The only real "blocked" scenario: strong trend opposing the only available direction
            let truly_blocked = match (&score.trading_direction, &score.price_regime) {
                // Strong directional move + very low ADX = conflicting signals, genuine confusion
                (TradingDirection::Neutral, PriceRegime::Ranging) if score.trend_strength < 12.0 => {
                    // Very flat, very weak — not enough signal for any strategy
                    true
                },
                _ => false,
            };
            
            if truly_blocked {
                blocked_opportunities.push(BlockedOpportunity {
                    instrument: symbol.to_string(),
                    strategy_category: "any".to_string(),
                    block_reason: format!(
                        "{} extremely flat (ADX={:.0}) — no directional or range signal",
                        symbol, score.trend_strength
                    ),
                    potential_direction: "NONE".to_string(),
                    adx_value: score.trend_strength,
                    price_regime: score.price_regime.as_str().to_string(),
                    estimated_miss: None,
                });
            }
        }
    }
    
    /// Analyze position performance relative to regime
    fn analyze_position_performance(
        &self,
        mrate: &MrateOutput,
        positions: &[PositionInfo],
        contradictions: &mut Vec<Contradiction>,
        suggestions: &mut Vec<Suggestion>,
    ) {
        if positions.is_empty() {
            return;
        }
        
        let now = Utc::now();
        let total_pnl: f64 = positions.iter().map(|p| p.unrealized_pnl).sum();
        let winning_positions: Vec<_> = positions.iter().filter(|p| p.unrealized_pnl > 0.0).collect();
        let losing_positions: Vec<_> = positions.iter().filter(|p| p.unrealized_pnl < 0.0).collect();
        
        // Check for favorable regime but losing positions
        if mrate.regime.is_strong() && total_pnl < -50.0 && losing_positions.len() > winning_positions.len() {
            contradictions.push(Contradiction {
                contradiction_type: ContradictionType::FavorableRegimeButLosing,
                instrument: "PORTFOLIO".to_string(),
                severity: "medium".to_string(),
                icon: "📉".to_string(),
                description: format!(
                    "Regime is {:?} (favorable) but portfolio showing ${:.2} loss",
                    mrate.regime, total_pnl.abs()
                ),
                evidence: vec![
                    format!("Regime: {:?}", mrate.regime),
                    format!("Total P&L: ${:.2}", total_pnl),
                    format!("Winning positions: {}", winning_positions.len()),
                    format!("Losing positions: {}", losing_positions.len()),
                ],
                detected_at: now,
            });
            
            suggestions.push(Suggestion {
                id: "regime-mismatch-review".to_string(),
                priority: SuggestionPriority::Medium,
                category: SuggestionCategory::RiskAlert,
                title: "Review regime classification".to_string(),
                description: format!(
                    "Current {:?} regime suggests favorable conditions but positions are net negative. Consider if regime classification is accurate.",
                    mrate.regime
                ),
                current_value: Some(format!("{:?}", mrate.regime)),
                suggested_value: None,
                expected_impact: "May need to adjust regime thresholds or input weightings".to_string(),
                confidence: 0.60,
                supporting_data: vec![
                    format!("Net P&L: ${:.2}", total_pnl),
                    format!("Loss ratio: {}/{}", losing_positions.len(), positions.len()),
                ],
                created_at: now,
            });
        }
    }
    
    /// Get historical performance by regime from the database
    async fn get_regime_performance(&self) -> Vec<RegimePerformance> {
        // Query trade history grouped by regime
        let result = sqlx::query!(
            r#"
            SELECT 
                mrate_regime,
                COUNT(*) as total,
                SUM(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) as wins,
                SUM(CASE WHEN pnl <= 0 THEN 1 ELSE 0 END) as losses,
                SUM(pnl) as total_pnl,
                AVG(CASE WHEN pnl > 0 THEN pnl END) as avg_win,
                AVG(CASE WHEN pnl < 0 THEN pnl END) as avg_loss
            FROM trade_context
            WHERE mrate_regime IS NOT NULL 
              AND exit_price IS NOT NULL
              AND closed_at > NOW() - INTERVAL '30 days'
            GROUP BY mrate_regime
            "#
        )
        .fetch_all(&self.pool)
        .await;
        
        match result {
            Ok(rows) => rows.iter().map(|row| {
                let total = row.total.unwrap_or(0) as i32;
                let wins = row.wins.unwrap_or(0) as i32;
                let losses = row.losses.unwrap_or(0) as i32;
                let win_rate = if total > 0 { wins as f64 / total as f64 } else { 0.0 };
                
                RegimePerformance {
                    regime: row.mrate_regime.clone().unwrap_or_default(),
                    total_trades: total,
                    winning_trades: wins,
                    losing_trades: losses,
                    total_pnl: row.total_pnl.as_ref().map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
                    avg_win: row.avg_win.as_ref().map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
                    avg_loss: row.avg_loss.as_ref().map(|d| d.to_string().parse().unwrap_or(0.0)).unwrap_or(0.0),
                    win_rate,
                }
            }).collect(),
            Err(e) => {
                tracing::warn!("Failed to fetch regime performance: {}", e);
                vec![]
            }
        }
    }
    
    /// Calculate overall health score based on contradictions and suggestions
    fn calculate_health_score(
        &self,
        contradictions: &[Contradiction],
        suggestions: &[Suggestion],
        performance: &[RegimePerformance],
    ) -> f64 {
        let mut score: f64 = 100.0;
        
        // Deduct for contradictions
        for c in contradictions {
            match c.severity.as_str() {
                "high" => score -= 15.0,
                "medium" => score -= 8.0,
                "low" => score -= 3.0,
                _ => score -= 5.0,
            }
        }
        
        // Deduct for high-priority suggestions
        for s in suggestions {
            match s.priority {
                SuggestionPriority::Critical => score -= 10.0,
                SuggestionPriority::High => score -= 5.0,
                SuggestionPriority::Medium => score -= 2.0,
                SuggestionPriority::Low => score -= 1.0,
            }
        }
        
        // Adjust based on historical performance
        let avg_win_rate: f64 = if !performance.is_empty() {
            performance.iter().map(|p| p.win_rate).sum::<f64>() / performance.len() as f64
        } else {
            0.5
        };
        
        if avg_win_rate > 0.55 {
            score += 10.0;
        } else if avg_win_rate < 0.45 {
            score -= 10.0;
        }
        
        score.clamp(0.0, 100.0)
    }
    
    /// Generate a human-readable summary
    fn generate_summary(&self, contradictions: &[Contradiction], suggestions: &[Suggestion], health_score: f64) -> String {
        if contradictions.is_empty() && suggestions.is_empty() {
            return "MRATE is operating well. No contradictions detected.".to_string();
        }
        
        let high_priority_count = suggestions.iter()
            .filter(|s| matches!(s.priority, SuggestionPriority::Critical | SuggestionPriority::High))
            .count();
        
        if health_score < 50.0 {
            format!(
                "⚠️ MRATE needs attention: {} contradictions detected, {} high-priority suggestions. Health score: {:.0}%",
                contradictions.len(), high_priority_count, health_score
            )
        } else if health_score < 75.0 {
            format!(
                "MRATE functioning with issues: {} contradictions, {} suggestions. Health: {:.0}%",
                contradictions.len(), suggestions.len(), health_score
            )
        } else {
            format!(
                "MRATE healthy ({:.0}%). {} minor items to review.",
                health_score, contradictions.len() + suggestions.len()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contradiction_severity() {
        assert_eq!(ContradictionType::TrendBlockedButTrending.severity(), "high");
        assert_eq!(ContradictionType::AvoidScoreButShortsProfit.severity(), "medium");
    }
    
    #[test]
    fn test_suggestion_priority_ordering() {
        assert!(SuggestionPriority::Critical < SuggestionPriority::High);
        assert!(SuggestionPriority::High < SuggestionPriority::Medium);
        assert!(SuggestionPriority::Medium < SuggestionPriority::Low);
    }
}
