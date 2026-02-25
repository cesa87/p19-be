//! Intelligence Gate — evaluates IntelligenceScore against a trade signal
//!
//! KEY DESIGN: The gate ALWAYS produces a decision and logs it.
//! Whether that decision is APPLIED depends on intelligence_gate_enabled per bot.
//! This means you can observe gate impact in bot activity logs before enabling it.

use serde::{Deserialize, Serialize};
use super::scorer::IntelligenceScore;
use super::settings::GateThresholds;

/// What the gate recommends for this trade signal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateAction {
    /// Pass through — tension is low or score confidence too low to act on
    Allow,
    /// Boost lot size (opportunity aligned with signal direction)
    Boost,
    /// Reduce lot size by 50% (elevated tension)
    ReduceSize,
    /// Block this trade entirely (high tension)
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceGateDecision {
    pub action: GateAction,
    /// Lot size multiplier to apply (1.0 = no change, 0.5 = halve, 1.25 = boost)
    pub size_multiplier: f64,
    pub tension_score: f64,
    pub opportunity_score: f64,
    pub confidence: f64,
    pub direction_bias: Option<String>,
    pub reason: String,
    /// Signal score 1-10 for display
    pub signal_score: u8,
}

impl IntelligenceGateDecision {
    /// A pass-through decision used when no intelligence score is available
    pub fn no_data() -> Self {
        Self {
            action: GateAction::Allow,
            size_multiplier: 1.0,
            tension_score: 50.0,
            opportunity_score: 50.0,
            confidence: 0.0,
            direction_bias: None,
            reason: "No intelligence score available — passing through".to_string(),
            signal_score: 5,
        }
    }
}

/// Evaluate the intelligence gate for a given trade signal direction.
///
/// `signal_is_long` — true if the bot is trying to go LONG, false for SHORT.
pub fn evaluate_gate(
    score: &IntelligenceScore,
    signal_is_long: bool,
    thresholds: &GateThresholds,
) -> IntelligenceGateDecision {
    // If confidence is too low, don't gate — we don't have enough data
    if score.confidence < thresholds.min_confidence {
        return IntelligenceGateDecision {
            action: GateAction::Allow,
            size_multiplier: 1.0,
            tension_score: score.tension_score,
            opportunity_score: score.opportunity_score,
            confidence: score.confidence,
            direction_bias: score.direction_bias.clone(),
            reason: format!(
                "🧠 Intel gate: confidence {:.0}% below threshold {:.0}% — pass through",
                score.confidence * 100.0,
                thresholds.min_confidence * 100.0
            ),
            signal_score: tension_to_signal_score(score.tension_score, score.opportunity_score),
        };
    }

    // Check if direction_bias conflicts with signal
    let direction_conflict = match score.direction_bias.as_deref() {
        Some("LONG") => !signal_is_long,
        Some("SHORT") => signal_is_long,
        _ => false,
    };

    // High tension → block regardless of direction
    if score.tension_score >= thresholds.block_tension {
        return IntelligenceGateDecision {
            action: GateAction::Block,
            size_multiplier: 0.0,
            tension_score: score.tension_score,
            opportunity_score: score.opportunity_score,
            confidence: score.confidence,
            direction_bias: score.direction_bias.clone(),
            reason: format!(
                "🚫 Intel gate BLOCK: tension {:.0}/100 ≥ threshold {:.0} (confidence {:.0}%)",
                score.tension_score, thresholds.block_tension, score.confidence * 100.0
            ),
            signal_score: tension_to_signal_score(score.tension_score, score.opportunity_score),
        };
    }

    // Elevated tension → reduce size
    if score.tension_score >= thresholds.reduce_tension || direction_conflict {
        let reason = if direction_conflict {
            format!(
                "⚠️ Intel gate REDUCE: direction bias {:?} conflicts with {} signal (tension {:.0})",
                score.direction_bias,
                if signal_is_long { "LONG" } else { "SHORT" },
                score.tension_score
            )
        } else {
            format!(
                "⚠️ Intel gate REDUCE: tension {:.0}/100 ≥ {:.0} — halving size",
                score.tension_score, thresholds.reduce_tension
            )
        };
        return IntelligenceGateDecision {
            action: GateAction::ReduceSize,
            size_multiplier: 0.5,
            tension_score: score.tension_score,
            opportunity_score: score.opportunity_score,
            confidence: score.confidence,
            direction_bias: score.direction_bias.clone(),
            reason,
            signal_score: tension_to_signal_score(score.tension_score, score.opportunity_score),
        };
    }

    // High opportunity + aligned direction → boost
    let direction_aligned = match score.direction_bias.as_deref() {
        Some("LONG") => signal_is_long,
        Some("SHORT") => !signal_is_long,
        _ => false,
    };

    if score.opportunity_score >= thresholds.boost_opportunity && direction_aligned {
        return IntelligenceGateDecision {
            action: GateAction::Boost,
            size_multiplier: 1.25,
            tension_score: score.tension_score,
            opportunity_score: score.opportunity_score,
            confidence: score.confidence,
            direction_bias: score.direction_bias.clone(),
            reason: format!(
                "🚀 Intel gate BOOST: opportunity {:.0}/100, bias {:?} aligned with {} (1.25x)",
                score.opportunity_score,
                score.direction_bias,
                if signal_is_long { "LONG" } else { "SHORT" }
            ),
            signal_score: tension_to_signal_score(score.tension_score, score.opportunity_score),
        };
    }

    // Default: allow unchanged
    IntelligenceGateDecision {
        action: GateAction::Allow,
        size_multiplier: 1.0,
        tension_score: score.tension_score,
        opportunity_score: score.opportunity_score,
        confidence: score.confidence,
        direction_bias: score.direction_bias.clone(),
        reason: format!(
            "✅ Intel gate ALLOW: tension {:.0}, opportunity {:.0}, confidence {:.0}%",
            score.tension_score, score.opportunity_score, score.confidence * 100.0
        ),
        signal_score: tension_to_signal_score(score.tension_score, score.opportunity_score),
    }
}

/// Convert tension/opportunity into a 1-10 signal score for display
fn tension_to_signal_score(tension: f64, opportunity: f64) -> u8 {
    // High opportunity + low tension = high score
    // High tension = low score
    let raw = (opportunity * 0.6 + (100.0 - tension) * 0.4) / 10.0;
    raw.clamp(1.0, 10.0).round() as u8
}

/// Map instrument name to intelligence score instrument key
pub fn instrument_to_intel_key(instrument: &str) -> &'static str {
    match instrument {
        "XAU_USD" => "XAU_USD",
        "BTC_USD" => "BTC_USD",
        "EUR_USD" => "EUR_USD",
        "USD_JPY" => "USD_JPY",
        "WTICO_USD" => "WTICO_USD",
        "NATGAS_USD" => "NATGAS_USD",
        _ => "XAU_USD", // fallback
    }
}
