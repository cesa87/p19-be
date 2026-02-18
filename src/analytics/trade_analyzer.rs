//! Trade Analyzer - AI-powered analysis of open positions
//!
//! Analyzes live trades against current MRATE conditions and generates
//! actionable recommendations (hold, close, scale out, add to position).

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::mrate::models::{MrateOutput, Regime, StrategyCategory};
use crate::broker::oanda::OandaClient;

/// Recommendation for an open trade
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeRecommendation {
    /// Hold position - conditions still favorable
    Hold,
    /// Consider closing - conditions deteriorating
    ConsiderClose,
    /// Close immediately - conditions reversed
    CloseNow,
    /// Scale out partial - lock in some profit
    ScaleOut { percent: f64 },
    /// Add to position - conditions strengthening
    AddToPosition,
}

impl TradeRecommendation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hold => "HOLD",
            Self::ConsiderClose => "CONSIDER_CLOSE",
            Self::CloseNow => "CLOSE_NOW",
            Self::ScaleOut { .. } => "SCALE_OUT",
            Self::AddToPosition => "ADD",
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Hold => "✅",
            Self::ConsiderClose => "⚠️",
            Self::CloseNow => "🚨",
            Self::ScaleOut { .. } => "📉",
            Self::AddToPosition => "📈",
        }
    }
    
    pub fn priority(&self) -> u8 {
        match self {
            Self::CloseNow => 1,
            Self::ConsiderClose => 2,
            Self::ScaleOut { .. } => 3,
            Self::Hold => 4,
            Self::AddToPosition => 5,
        }
    }
}

/// Analysis result for a single trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeAnalysis {
    pub trade_id: String,
    pub bot_id: Uuid,
    pub bot_name: String,
    pub instrument: String,
    pub direction: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
    pub units: f64,
    pub opened_at: DateTime<Utc>,
    pub time_in_trade_minutes: i64,
    
    // AI Analysis
    pub recommendation: TradeRecommendation,
    pub confidence: f64,
    pub reasons: Vec<String>,
    
    // Regime alignment
    pub entry_regime: Option<String>,
    pub current_regime: String,
    pub regime_aligned: bool,
    
    // Technical signals
    pub trend_direction: String,
    pub momentum_score: f64,
    pub reversal_probability: f64,
    
    // Risk metrics
    pub risk_reward_current: f64,
    pub max_favorable_excursion: f64,
    pub max_adverse_excursion: f64,
}

/// Aggregated insights from all open trades
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradeInsights {
    pub total_open_trades: usize,
    pub total_unrealized_pnl: f64,
    pub net_direction_bias: f64,  // -1.0 (all short) to +1.0 (all long)
    pub regime_alignment_pct: f64,  // % of trades aligned with current regime
    pub avg_time_in_trade_minutes: f64,
    pub trades_needing_attention: usize,  // CloseNow or ConsiderClose
    pub dominant_instrument: Option<String>,
    pub position_concentration: f64,  // 0-1, how concentrated in one instrument
}

/// The Trade Analyzer engine
pub struct TradeAnalyzer;

impl TradeAnalyzer {
    /// Analyze all open positions against current market conditions
    pub async fn analyze_open_trades(
        pool: &PgPool,
        oanda: &OandaClient,
        mrate: &MrateOutput,
    ) -> Result<(Vec<TradeAnalysis>, TradeInsights), String> {
        // Get open trades from OANDA
        let open_trades = oanda.get_open_trades().await
            .map_err(|e| format!("Failed to get open trades: {}", e))?;
        
        if open_trades.is_empty() {
            return Ok((vec![], TradeInsights::default()));
        }
        
        // Get bot info for each trade
        let mut analyses = Vec::new();
        let mut total_pnl = 0.0;
        let mut long_exposure = 0.0;
        let mut short_exposure = 0.0;
        let mut aligned_count = 0;
        let mut attention_count = 0;
        let mut total_time = 0i64;
        let mut instrument_exposure: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        
        for trade in &open_trades {
            // Parse trade data
            let units: f64 = trade.current_units.parse().unwrap_or(0.0);
            let entry_price: f64 = trade.price.parse().unwrap_or(0.0);
            let unrealized_pnl: f64 = trade.unrealized_pl
                .as_ref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let is_long = units > 0.0;
            let direction = if is_long { "LONG" } else { "SHORT" };
            
            // Get current price (mid price from bid/ask)
            let current_price = oanda.get_price(&trade.instrument).await
                .map(|p| {
                    let bid: f64 = p.bids.first()
                        .and_then(|b| b.price.parse().ok())
                        .unwrap_or(0.0);
                    let ask: f64 = p.asks.first()
                        .and_then(|a| a.price.parse().ok())
                        .unwrap_or(0.0);
                    (bid + ask) / 2.0
                })
                .unwrap_or(entry_price);
            
            // Calculate P&L percentage
            let pnl_pct = if entry_price > 0.0 {
                if is_long {
                    ((current_price - entry_price) / entry_price) * 100.0
                } else {
                    ((entry_price - current_price) / entry_price) * 100.0
                }
            } else {
                0.0
            };
            
            // Parse open time
            let opened_at = chrono::DateTime::parse_from_rfc3339(&trade.open_time)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let time_in_trade = (Utc::now() - opened_at).num_minutes();
            
            // Look up bot info
            let bot_info = sqlx::query!(
                r#"
                SELECT b.id, b.name, s.mrate_category
                FROM bots b
                LEFT JOIN strategies s ON b.strategy_id = s.id
                WHERE b.name ILIKE '%' || $1 || '%'
                LIMIT 1
                "#,
                trade.instrument.replace("_", "")
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
            
            let bot_id = bot_info.as_ref().map(|b| b.id).unwrap_or(Uuid::nil());
            let bot_name = bot_info.as_ref().map(|b| b.name.clone()).unwrap_or_else(|| "Unknown".to_string());
            let mrate_category = bot_info.as_ref()
                .and_then(|b| b.mrate_category.as_ref())
                .and_then(|c| StrategyCategory::from_str(c));
            
            // Analyze against current conditions
            let (recommendation, confidence, reasons, regime_aligned) = Self::analyze_trade(
                &trade.instrument,
                direction,
                pnl_pct,
                time_in_trade,
                mrate,
                mrate_category,
            );
            
            // Calculate trend and reversal signals
            let (trend_direction, momentum_score, reversal_prob) = Self::calculate_technical_signals(
                &trade.instrument,
                direction,
                mrate,
            );
            
            // Track aggregates
            total_pnl += unrealized_pnl;
            if is_long {
                long_exposure += units.abs() * current_price;
            } else {
                short_exposure += units.abs() * current_price;
            }
            if regime_aligned {
                aligned_count += 1;
            }
            if recommendation == TradeRecommendation::CloseNow || recommendation == TradeRecommendation::ConsiderClose {
                attention_count += 1;
            }
            total_time += time_in_trade;
            *instrument_exposure.entry(trade.instrument.clone()).or_insert(0.0) += units.abs() * current_price;
            
            analyses.push(TradeAnalysis {
                trade_id: trade.id.clone(),
                bot_id,
                bot_name,
                instrument: trade.instrument.clone(),
                direction: direction.to_string(),
                entry_price,
                current_price,
                unrealized_pnl,
                unrealized_pnl_pct: pnl_pct,
                units: units.abs(),
                opened_at,
                time_in_trade_minutes: time_in_trade,
                recommendation,
                confidence,
                reasons,
                entry_regime: None, // Would need to store this when trade opens
                current_regime: mrate.regime.as_str().to_string(),
                regime_aligned,
                trend_direction,
                momentum_score,
                reversal_probability: reversal_prob,
                risk_reward_current: pnl_pct.abs() / 1.0, // Simplified
                max_favorable_excursion: pnl_pct.max(0.0),
                max_adverse_excursion: pnl_pct.min(0.0).abs(),
            });
        }
        
        // Sort by priority (most urgent first)
        analyses.sort_by_key(|a| a.recommendation.priority());
        
        // Calculate insights
        let total_exposure = long_exposure + short_exposure;
        let net_bias = if total_exposure > 0.0 {
            (long_exposure - short_exposure) / total_exposure
        } else {
            0.0
        };
        
        let dominant = instrument_exposure.iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| k.clone());
        
        let concentration = if total_exposure > 0.0 {
            instrument_exposure.values().cloned().fold(0.0_f64, f64::max) / total_exposure
        } else {
            0.0
        };
        
        let insights = TradeInsights {
            total_open_trades: analyses.len(),
            total_unrealized_pnl: total_pnl,
            net_direction_bias: net_bias,
            regime_alignment_pct: if analyses.is_empty() { 100.0 } else { (aligned_count as f64 / analyses.len() as f64) * 100.0 },
            avg_time_in_trade_minutes: if analyses.is_empty() { 0.0 } else { total_time as f64 / analyses.len() as f64 },
            trades_needing_attention: attention_count,
            dominant_instrument: dominant,
            position_concentration: concentration,
        };
        
        Ok((analyses, insights))
    }
    
    /// Analyze a single trade against current conditions
    fn analyze_trade(
        instrument: &str,
        direction: &str,
        pnl_pct: f64,
        time_in_trade: i64,
        mrate: &MrateOutput,
        category: Option<StrategyCategory>,
    ) -> (TradeRecommendation, f64, Vec<String>, bool) {
        let mut reasons = Vec::new();
        let mut score: f64 = 50.0; // Neutral starting point
        let is_long = direction == "LONG";
        
        // 1. Check regime alignment
        let regime_favors_direction = match mrate.regime {
            Regime::Trend => true, // Trend favors directional trades
            Regime::Choppy => false, // Choppy doesn't favor holding
            Regime::GoldSuperBull => is_long && instrument.contains("XAU"),
            Regime::BtcSuperBull => is_long && instrument.contains("BTC"),
            Regime::Panic => !is_long, // Panic favors shorts
        };
        
        let regime_aligned = regime_favors_direction;
        if regime_aligned {
            score += 15.0;
            reasons.push(format!("✅ {} regime supports {} position", mrate.regime.as_str(), direction));
        } else {
            score -= 20.0;
            reasons.push(format!("⚠️ {} regime may not favor {} position", mrate.regime.as_str(), direction));
        }
        
        // 2. Check strategy category alignment
        if let Some(cat) = category {
            let weight = mrate.strategy_weights.get(cat);
            if weight >= 0.7 {
                score += 10.0;
                reasons.push(format!("✅ MRATE favors {:?} strategies ({:.0}%)", cat, weight * 100.0));
            } else if weight < 0.3 {
                score -= 15.0;
                reasons.push(format!("⚠️ MRATE disfavors {:?} strategies ({:.0}%)", cat, weight * 100.0));
            }
        }
        
        // 3. Check P&L status
        if pnl_pct > 2.0 {
            score += 10.0;
            reasons.push(format!("📈 In profit +{:.2}%", pnl_pct));
        } else if pnl_pct < -2.0 {
            score -= 15.0;
            reasons.push(format!("📉 In loss {:.2}%", pnl_pct));
        }
        
        // 4. Check time in trade (mean reversion trades should close faster)
        if time_in_trade > 480 { // 8 hours
            score -= 5.0;
            reasons.push(format!("⏰ Trade open for {} hours", time_in_trade / 60));
        }
        
        // 5. Check uncertainty
        if mrate.uncertainty_score > 70.0 {
            score -= 10.0;
            reasons.push(format!("⚠️ High market uncertainty ({:.0})", mrate.uncertainty_score));
        }
        
        // 6. Check if favored instrument matches
        let favored_str = mrate.favored_instrument.as_str();
        let favors_this = match mrate.favored_instrument {
            crate::mrate::models::FavoredInstrument::Gold => instrument.contains("XAU"),
            crate::mrate::models::FavoredInstrument::Bitcoin => instrument.contains("BTC"),
            crate::mrate::models::FavoredInstrument::Both => instrument.contains("XAU") || instrument.contains("BTC"),
            crate::mrate::models::FavoredInstrument::Neither => false,
        };
        if favors_this {
            score += 10.0;
            reasons.push(format!("✅ {} is MRATE favored instrument", favored_str));
        }
        
        // 7. Check momentum
        let liquidity_momentum = mrate.liquidity_momentum;
        let risk_momentum = mrate.risk_momentum;
        
        if is_long && liquidity_momentum > 0.0 && risk_momentum < 0.0 {
            score += 8.0;
            reasons.push("📊 Momentum supports long positions".to_string());
        } else if !is_long && liquidity_momentum < 0.0 && risk_momentum > 0.0 {
            score += 8.0;
            reasons.push("📊 Momentum supports short positions".to_string());
        } else if (is_long && liquidity_momentum < -0.5) || (!is_long && liquidity_momentum > 0.5) {
            score -= 10.0;
            reasons.push("📊 Momentum moving against position".to_string());
        }
        
        // Determine recommendation
        let confidence: f64 = (score / 100.0).clamp(0.0, 1.0);
        let recommendation = if score >= 70.0 {
            TradeRecommendation::Hold
        } else if score >= 55.0 && pnl_pct > 1.5 {
            TradeRecommendation::ScaleOut { percent: 50.0 }
        } else if score >= 40.0 {
            TradeRecommendation::Hold
        } else if score >= 25.0 {
            TradeRecommendation::ConsiderClose
        } else {
            TradeRecommendation::CloseNow
        };
        
        (recommendation, confidence, reasons, regime_aligned)
    }
    
    /// Calculate technical signals for a trade
    fn calculate_technical_signals(
        instrument: &str,
        direction: &str,
        mrate: &MrateOutput,
    ) -> (String, f64, f64) {
        // Use MRATE inputs to determine trend
        let inputs = mrate.inputs.as_ref();
        
        // Helper to convert TrendDirection to string
        fn trend_to_str(t: &crate::mrate::models::TrendDirection) -> &'static str {
            use crate::mrate::models::TrendDirection;
            match t {
                TrendDirection::StrongUp => "STRONG_UP",
                TrendDirection::Up => "UP",
                TrendDirection::Flat => "FLAT",
                TrendDirection::Down => "DOWN",
                TrendDirection::StrongDown => "STRONG_DOWN",
            }
        }
        
        fn trend_inverse(t: &crate::mrate::models::TrendDirection) -> &'static str {
            use crate::mrate::models::TrendDirection;
            match t {
                TrendDirection::StrongUp => "STRONG_DOWN",
                TrendDirection::Up => "DOWN",
                TrendDirection::Flat => "FLAT",
                TrendDirection::Down => "UP",
                TrendDirection::StrongDown => "STRONG_UP",
            }
        }
        
        let trend = if instrument.contains("XAU") {
            inputs.and_then(|i| i.sp500_trend.as_ref())
                .map(|t| trend_to_str(t))
                .unwrap_or("FLAT")
        } else if instrument.contains("BTC") {
            inputs.and_then(|i| i.btc_trend.as_ref())
                .map(|t| trend_to_str(t))
                .unwrap_or("FLAT")
        } else if instrument.contains("EUR") || instrument.contains("GBP") {
            // Inverse of DXY for forex
            inputs.and_then(|i| i.dxy_trend.as_ref())
                .map(|t| trend_inverse(t))
                .unwrap_or("FLAT")
        } else {
            "FLAT"
        };
        
        // Momentum score based on liquidity/risk momentum
        let momentum = (mrate.liquidity_momentum - mrate.risk_momentum) / 2.0;
        let momentum_score = (50.0 + momentum * 25.0).clamp(0.0, 100.0);
        
        // Reversal probability based on uncertainty and extreme readings
        let reversal_prob = if mrate.uncertainty_score > 60.0 {
            (mrate.uncertainty_score / 100.0) * 0.5
        } else {
            0.1
        };
        
        (trend.to_string(), momentum_score, reversal_prob)
    }
    
    /// Generate MRATE feedback from trade analysis
    pub fn generate_mrate_feedback(insights: &TradeInsights, analyses: &[TradeAnalysis]) -> MrateTradeFeedback {
        let mut feedback = MrateTradeFeedback::default();
        
        feedback.has_open_positions = insights.total_open_trades > 0;
        feedback.total_unrealized_pnl = insights.total_unrealized_pnl;
        feedback.net_direction_bias = insights.net_direction_bias;
        feedback.regime_alignment_score = insights.regime_alignment_pct;
        
        // Count positions needing attention
        feedback.positions_at_risk = analyses.iter()
            .filter(|a| matches!(a.recommendation, TradeRecommendation::CloseNow | TradeRecommendation::ConsiderClose))
            .count();
        
        // Suggest regime confidence adjustment
        if insights.trades_needing_attention > insights.total_open_trades / 2 {
            feedback.suggested_confidence_adjustment = -0.1; // Reduce confidence
            feedback.feedback_reason = Some("Majority of positions misaligned with conditions".to_string());
        } else if insights.regime_alignment_pct > 80.0 && insights.total_unrealized_pnl > 0.0 {
            feedback.suggested_confidence_adjustment = 0.05; // Slight boost
            feedback.feedback_reason = Some("Positions well-aligned and profitable".to_string());
        }
        
        feedback
    }
}

/// Feedback from trade analysis to inform MRATE
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MrateTradeFeedback {
    pub has_open_positions: bool,
    pub total_unrealized_pnl: f64,
    pub net_direction_bias: f64,
    pub regime_alignment_score: f64,
    pub positions_at_risk: usize,
    pub suggested_confidence_adjustment: f64,
    pub feedback_reason: Option<String>,
}
