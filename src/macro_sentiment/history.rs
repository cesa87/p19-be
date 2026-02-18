//! Macro History Service
//! 
//! Stores Polymarket probability snapshots and calculates 7-day deltas
//! for the Macro-Aligned Momentum strategy.

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, FromRow};
use tracing::{info, warn};
use uuid::Uuid;

use super::MacroSentiment;

/// Macro regime classification
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MacroRegime {
    RiskOn,   // Score > 15, bullish, favor longs
    RiskOff,  // Score < -15, bearish, favor shorts
    Neutral,  // Between -15 and 15, reduced trading
}

impl std::fmt::Display for MacroRegime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MacroRegime::RiskOn => write!(f, "RISK_ON"),
            MacroRegime::RiskOff => write!(f, "RISK_OFF"),
            MacroRegime::Neutral => write!(f, "NEUTRAL"),
        }
    }
}

impl From<&str> for MacroRegime {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "RISK_ON" => MacroRegime::RiskOn,
            "RISK_OFF" => MacroRegime::RiskOff,
            _ => MacroRegime::Neutral,
        }
    }
}

/// A snapshot of macro probabilities at a point in time
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MacroSnapshot {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub rate_cut_prob: f64,
    pub rate_hike_prob: f64,
    pub inflation_prob: f64,
    pub recession_prob: f64,
    pub btc_sentiment: f64,
    pub macro_score: f64,
    pub macro_regime: String,
    pub position_multiplier: f64,
    pub breakout_mode: bool,
    pub regime_flip_date: Option<DateTime<Utc>>,
    pub markets_analyzed: i32,
    pub created_at: DateTime<Utc>,
}

/// The computed macro regime state with deltas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroRegimeState {
    /// Current macro score (-100 to +100)
    pub score: f64,
    /// Current regime
    pub regime: MacroRegime,
    /// Position size multiplier (1.0, 1.5, or 2.0)
    pub position_multiplier: f64,
    /// Is breakout mode active?
    pub breakout_mode: bool,
    /// Days since last regime flip
    pub days_since_flip: Option<i64>,
    
    // Current raw probabilities
    pub rate_cut_prob: f64,
    pub inflation_prob: f64,
    pub recession_prob: f64,
    pub btc_sentiment: f64,
    
    // 7-day deltas (percentage points)
    pub delta_rate_cut: f64,
    pub delta_inflation: f64,
    pub delta_recession: f64,
    pub delta_btc: f64,
    
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
    /// Number of markets used in calculation
    pub markets_analyzed: i32,
}

impl Default for MacroRegimeState {
    fn default() -> Self {
        Self {
            score: 0.0,
            regime: MacroRegime::Neutral,
            position_multiplier: 1.0,
            breakout_mode: false,
            days_since_flip: None,
            rate_cut_prob: 0.0,
            inflation_prob: 0.5,
            recession_prob: 0.5,
            btc_sentiment: 0.0,
            delta_rate_cut: 0.0,
            delta_inflation: 0.0,
            delta_recession: 0.0,
            delta_btc: 0.0,
            last_updated: Utc::now(),
            markets_analyzed: 0,
        }
    }
}

/// Macro History Service
pub struct MacroHistoryService {
    pool: PgPool,
}

impl MacroHistoryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Save a new macro snapshot from current Polymarket sentiment
    pub async fn save_snapshot(&self, sentiment: &MacroSentiment) -> Result<Uuid, String> {
        // First get the previous state to check for regime flip
        let previous = self.get_latest_snapshot().await.ok();
        let previous_regime = previous.as_ref()
            .map(|p| MacroRegime::from(p.macro_regime.as_str()))
            .unwrap_or(MacroRegime::Neutral);
        
        // Calculate current state
        let state = self.calculate_regime_state_internal(sentiment, previous.as_ref()).await;
        
        // Check for regime flip
        let (breakout_mode, regime_flip_date) = if state.regime != previous_regime && previous.is_some() {
            info!("Macro regime flipped from {:?} to {:?}", previous_regime, state.regime);
            (true, Some(Utc::now()))
        } else if let Some(prev) = &previous {
            // Continue breakout mode for 10 days after flip
            let flip_date = prev.regime_flip_date;
            let still_in_breakout = flip_date
                .map(|fd| (Utc::now() - fd).num_days() < 10)
                .unwrap_or(false);
            (still_in_breakout, flip_date)
        } else {
            (false, None)
        };
        
        let id = Uuid::new_v4();
        let now = Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO macro_history (
                id, timestamp, rate_cut_prob, rate_hike_prob, inflation_prob,
                recession_prob, btc_sentiment, macro_score, macro_regime,
                position_multiplier, breakout_mode, regime_flip_date, markets_analyzed
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#
        )
        .bind(id)
        .bind(now)
        .bind(sentiment.fed_rate_cut_probability)
        .bind(sentiment.fed_rate_hike_probability)
        .bind(sentiment.inflation_expectation)
        .bind(sentiment.safe_haven_demand)
        .bind(sentiment.btc_sentiment_score)
        .bind(state.score)
        .bind(state.regime.to_string())
        .bind(state.position_multiplier)
        .bind(breakout_mode)
        .bind(regime_flip_date)
        .bind(sentiment.markets_analyzed as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to save macro snapshot: {}", e))?;
        
        info!(
            "Saved macro snapshot: score={:.1}, regime={:?}, multiplier={:.1}x, breakout={}",
            state.score, state.regime, state.position_multiplier, breakout_mode
        );
        
        Ok(id)
    }
    
    /// Get the latest macro snapshot
    pub async fn get_latest_snapshot(&self) -> Result<MacroSnapshot, String> {
        sqlx::query_as::<_, MacroSnapshot>(
            r#"
            SELECT 
                id, timestamp, rate_cut_prob, rate_hike_prob, inflation_prob,
                recession_prob, btc_sentiment, macro_score, macro_regime,
                position_multiplier, breakout_mode, regime_flip_date,
                markets_analyzed, created_at
            FROM macro_history
            ORDER BY timestamp DESC
            LIMIT 1
            "#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to get latest snapshot: {}", e))
    }
    
    /// Get snapshot from approximately 7 days ago
    pub async fn get_week_ago_snapshot(&self) -> Result<Option<MacroSnapshot>, String> {
        let week_ago = Utc::now() - Duration::days(7);
        
        sqlx::query_as::<_, MacroSnapshot>(
            r#"
            SELECT 
                id, timestamp, rate_cut_prob, rate_hike_prob, inflation_prob,
                recession_prob, btc_sentiment, macro_score, macro_regime,
                position_multiplier, breakout_mode, regime_flip_date,
                markets_analyzed, created_at
            FROM macro_history
            WHERE timestamp <= $1
            ORDER BY timestamp DESC
            LIMIT 1
            "#
        )
        .bind(week_ago)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Failed to get week-ago snapshot: {}", e))
    }
    
    /// Calculate the current macro regime state with deltas
    pub async fn calculate_regime_state(&self) -> MacroRegimeState {
        let latest = match self.get_latest_snapshot().await {
            Ok(s) => s,
            Err(e) => {
                warn!("No macro history available: {}", e);
                return MacroRegimeState::default();
            }
        };
        
        let week_ago = self.get_week_ago_snapshot().await.ok().flatten();
        
        self.compute_state_from_snapshots(&latest, week_ago.as_ref())
    }
    
    /// Internal calculation of regime state
    async fn calculate_regime_state_internal(
        &self,
        sentiment: &MacroSentiment,
        previous: Option<&MacroSnapshot>,
    ) -> MacroRegimeState {
        let week_ago = self.get_week_ago_snapshot().await.ok().flatten();
        
        // Calculate deltas
        let (delta_rate_cut, delta_inflation, delta_recession, delta_btc) = 
            if let Some(wa) = &week_ago {
                (
                    (sentiment.fed_rate_cut_probability - wa.rate_cut_prob) * 100.0,
                    (sentiment.inflation_expectation - wa.inflation_prob) * 100.0,
                    (sentiment.safe_haven_demand - wa.recession_prob) * 100.0,
                    (sentiment.btc_sentiment_score - wa.btc_sentiment) * 100.0,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
        
        // Calculate macro score
        let score = delta_rate_cut + delta_inflation - (delta_recession * 0.5);
        
        // Determine regime
        let regime = if score > 15.0 {
            MacroRegime::RiskOn
        } else if score < -15.0 {
            MacroRegime::RiskOff
        } else {
            MacroRegime::Neutral
        };
        
        // Calculate position multiplier
        let position_multiplier = if score.abs() > 40.0 {
            2.0
        } else if score.abs() > 25.0 {
            1.5
        } else {
            1.0
        };
        
        // Check breakout mode
        let (breakout_mode, days_since_flip) = if let Some(prev) = previous {
            let prev_regime = MacroRegime::from(prev.macro_regime.as_str());
            if regime != prev_regime {
                (true, Some(0))
            } else if let Some(flip_date) = prev.regime_flip_date {
                let days = (Utc::now() - flip_date).num_days();
                (days < 10, Some(days))
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };
        
        MacroRegimeState {
            score,
            regime,
            position_multiplier,
            breakout_mode,
            days_since_flip,
            rate_cut_prob: sentiment.fed_rate_cut_probability,
            inflation_prob: sentiment.inflation_expectation,
            recession_prob: sentiment.safe_haven_demand,
            btc_sentiment: sentiment.btc_sentiment_score,
            delta_rate_cut,
            delta_inflation,
            delta_recession,
            delta_btc,
            last_updated: Utc::now(),
            markets_analyzed: sentiment.markets_analyzed as i32,
        }
    }
    
    /// Compute state from database snapshots
    fn compute_state_from_snapshots(
        &self,
        latest: &MacroSnapshot,
        week_ago: Option<&MacroSnapshot>,
    ) -> MacroRegimeState {
        // Calculate deltas
        let (delta_rate_cut, delta_inflation, delta_recession, delta_btc) = 
            if let Some(wa) = week_ago {
                (
                    (latest.rate_cut_prob - wa.rate_cut_prob) * 100.0,
                    (latest.inflation_prob - wa.inflation_prob) * 100.0,
                    (latest.recession_prob - wa.recession_prob) * 100.0,
                    (latest.btc_sentiment - wa.btc_sentiment) * 100.0,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
        
        // Calculate macro score
        let score = delta_rate_cut + delta_inflation - (delta_recession * 0.5);
        
        // Determine regime
        let regime = if score > 15.0 {
            MacroRegime::RiskOn
        } else if score < -15.0 {
            MacroRegime::RiskOff
        } else {
            MacroRegime::Neutral
        };
        
        // Calculate position multiplier
        let position_multiplier = if score.abs() > 40.0 {
            2.0
        } else if score.abs() > 25.0 {
            1.5
        } else {
            1.0
        };
        
        // Breakout mode
        let days_since_flip = latest.regime_flip_date
            .map(|fd| (Utc::now() - fd).num_days());
        let breakout_mode = days_since_flip.map(|d| d < 10).unwrap_or(false);
        
        MacroRegimeState {
            score,
            regime,
            position_multiplier,
            breakout_mode,
            days_since_flip,
            rate_cut_prob: latest.rate_cut_prob,
            inflation_prob: latest.inflation_prob,
            recession_prob: latest.recession_prob,
            btc_sentiment: latest.btc_sentiment,
            delta_rate_cut,
            delta_inflation,
            delta_recession,
            delta_btc,
            last_updated: latest.timestamp,
            markets_analyzed: latest.markets_analyzed,
        }
    }
    
    /// Clean up old snapshots (keep last 90 days)
    pub async fn cleanup_old_snapshots(&self) -> Result<u64, String> {
        let cutoff = Utc::now() - Duration::days(90);
        
        let result = sqlx::query("DELETE FROM macro_history WHERE timestamp < $1")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(|e| format!("Failed to cleanup old snapshots: {}", e))?;
        
        let deleted = result.rows_affected();
        if deleted > 0 {
            info!("Cleaned up {} old macro history snapshots", deleted);
        }
        
        Ok(deleted)
    }
}

/// Calculate regime state without database (for testing or when DB unavailable)
pub fn calculate_regime_from_sentiment(
    current: &MacroSentiment,
    week_ago: Option<&MacroSentiment>,
) -> MacroRegimeState {
    let (delta_rate_cut, delta_inflation, delta_recession, delta_btc) = 
        if let Some(wa) = week_ago {
            (
                (current.fed_rate_cut_probability - wa.fed_rate_cut_probability) * 100.0,
                (current.inflation_expectation - wa.inflation_expectation) * 100.0,
                (current.safe_haven_demand - wa.safe_haven_demand) * 100.0,
                (current.btc_sentiment_score - wa.btc_sentiment_score) * 100.0,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
    
    let score = delta_rate_cut + delta_inflation - (delta_recession * 0.5);
    
    let regime = if score > 15.0 {
        MacroRegime::RiskOn
    } else if score < -15.0 {
        MacroRegime::RiskOff
    } else {
        MacroRegime::Neutral
    };
    
    let position_multiplier = if score.abs() > 40.0 {
        2.0
    } else if score.abs() > 25.0 {
        1.5
    } else {
        1.0
    };
    
    MacroRegimeState {
        score,
        regime,
        position_multiplier,
        breakout_mode: false,
        days_since_flip: None,
        rate_cut_prob: current.fed_rate_cut_probability,
        inflation_prob: current.inflation_expectation,
        recession_prob: current.safe_haven_demand,
        btc_sentiment: current.btc_sentiment_score,
        delta_rate_cut,
        delta_inflation,
        delta_recession,
        delta_btc,
        last_updated: Utc::now(),
        markets_analyzed: current.markets_analyzed as i32,
    }
}
