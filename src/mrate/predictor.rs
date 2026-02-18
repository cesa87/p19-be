//! Regime Transition Predictor
//! 
//! Predicts upcoming MRATE regime changes using historical patterns.
//! Uses a lightweight gradient-based model trained on MRATE score transitions.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};

use super::models::Regime;

/// Regime transition prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimePrediction {
    pub current_regime: Regime,
    pub predicted_regime: Regime,
    pub probability: f64, // 0.0-1.0
    pub horizon_hours: i32, // How far ahead
    pub confidence: String, // "low", "medium", "high"
    pub reasoning: Vec<String>,
}

/// Training sample for regime prediction
#[derive(Debug, Clone)]
struct TransitionSample {
    from_regime: String,
    to_regime: String,
    liq_delta: f64,
    risk_delta: f64,
    unc_delta: f64,
    liq_momentum: f64,
    risk_momentum: f64,
    vix_level: Option<f64>,
    fear_greed: Option<f64>,
    time_to_transition_hours: f64,
}

/// Simple weighted score model for regime prediction
pub struct RegimePredictor {
    pool: PgPool,
}

impl RegimePredictor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Record current MRATE state for training
    pub async fn record_snapshot(
        &self,
        regime: Regime,
        liquidity_score: f64,
        risk_score: f64,
        uncertainty_score: f64,
        liquidity_momentum: f64,
        risk_momentum: f64,
        vix_level: Option<f64>,
        dxy_level: Option<f64>,
        fear_greed_index: Option<f64>,
        btc_trend: Option<String>,
        sp500_trend: Option<String>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO regime_snapshots 
                (timestamp, regime, liquidity_score, risk_score, uncertainty_score,
                 liquidity_momentum, risk_momentum, vix_level, dxy_level, 
                 fear_greed_index, btc_trend, sp500_trend)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
            Utc::now().naive_utc(),
            regime.as_str(),
            liquidity_score as f32,
            risk_score as f32,
            uncertainty_score as f32,
            liquidity_momentum as f32,
            risk_momentum as f32,
            vix_level.map(|v| v as f32),
            dxy_level.map(|v| v as f32),
            fear_greed_index.map(|v| v as f32),
            btc_trend,
            sp500_trend
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Update past snapshots with actual transition outcomes (for supervised learning)
    pub async fn label_transitions(&self) -> Result<usize> {
        // Find regime changes and backfill "next_regime" for training
        let updated = sqlx::query!(
            r#"
            WITH transitions AS (
                SELECT 
                    r1.id as snapshot_id,
                    r2.regime as next_regime,
                    r2.timestamp as next_timestamp
                FROM regime_snapshots r1
                JOIN regime_snapshots r2 ON r2.timestamp = (
                    SELECT MIN(timestamp) 
                    FROM regime_snapshots 
                    WHERE timestamp > r1.timestamp AND regime != r1.regime
                )
                WHERE r1.next_regime IS NULL
                  AND r1.timestamp > NOW() - INTERVAL '30 days'
            )
            UPDATE regime_snapshots r
            SET next_regime = t.next_regime,
                next_regime_timestamp = t.next_timestamp
            FROM transitions t
            WHERE r.id = t.snapshot_id
            "#
        )
        .execute(&self.pool)
        .await?;
        
        if updated.rows_affected() > 0 {
            info!("Predictor: Labeled {} regime transitions for training", updated.rows_affected());
        }
        
        Ok(updated.rows_affected() as usize)
    }
    
    /// Predict next regime change
    pub async fn predict(
        &self,
        current_regime: Regime,
        liquidity_score: f64,
        risk_score: f64,
        uncertainty_score: f64,
        liquidity_momentum: f64,
        risk_momentum: f64,
        vix_level: Option<f64>,
        fear_greed: Option<f64>,
    ) -> Result<Option<RegimePrediction>> {
        // Fetch historical transitions for pattern matching
        let samples = self.fetch_training_samples().await?;
        
        if samples.len() < 20 {
            // Not enough data yet
            return Ok(None);
        }
        
        // Score each possible regime transition
        let mut predictions: Vec<(Regime, f64, Vec<String>)> = vec![];
        
        for target_regime in [
            Regime::GoldSuperBull,
            Regime::BtcSuperBull,
            Regime::Panic,
            Regime::Trend,
            Regime::Choppy,
        ] {
            if target_regime == current_regime {
                continue;
            }
            
            let (score, reasons) = self.score_transition(
                &current_regime,
                &target_regime,
                liquidity_score,
                risk_score,
                uncertainty_score,
                liquidity_momentum,
                risk_momentum,
                vix_level,
                fear_greed,
                &samples,
            );
            
            predictions.push((target_regime, score, reasons));
        }
        
        // Sort by score
        predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let (predicted_regime, probability, reasoning) = &predictions[0];
        
        // Only return prediction if probability > 0.5
        if *probability < 0.5 {
            return Ok(None);
        }
        
        let confidence = if *probability > 0.75 {
            "high"
        } else if *probability > 0.6 {
            "medium"
        } else {
            "low"
        };
        
        // Estimate time horizon based on momentum strength
        let momentum_strength = (liquidity_momentum.abs() + risk_momentum.abs()) / 2.0;
        let horizon_hours = if momentum_strength > 15.0 {
            1 // Fast transition
        } else if momentum_strength > 8.0 {
            2
        } else {
            4
        };
        
        info!(
            "Predictor: {}→{} predicted with {:.0}% probability in ~{}h",
            current_regime.as_str(),
            predicted_regime.as_str(),
            probability * 100.0,
            horizon_hours
        );
        
        Ok(Some(RegimePrediction {
            current_regime,
            predicted_regime: *predicted_regime,
            probability: *probability,
            horizon_hours,
            confidence: confidence.to_string(),
            reasoning: reasoning.clone(),
        }))
    }
    
    /// Score a potential regime transition
    fn score_transition(
        &self,
        from: &Regime,
        to: &Regime,
        liq: f64,
        risk: f64,
        unc: f64,
        liq_mom: f64,
        risk_mom: f64,
        vix: Option<f64>,
        fg: Option<f64>,
        samples: &[TransitionSample],
    ) -> (f64, Vec<String>) {
        let mut score = 0.0;
        let mut reasons = Vec::new();
        
        // Filter samples for this specific transition
        let relevant_samples: Vec<_> = samples
            .iter()
            .filter(|s| s.from_regime == from.as_str() && s.to_regime == to.as_str())
            .collect();
        
        if relevant_samples.is_empty() {
            // No historical precedent - use heuristics
            return self.heuristic_score(from, to, liq, risk, unc, liq_mom, risk_mom, vix, fg);
        }
        
        // Calculate average deltas from samples
        let avg_liq_delta: f64 = relevant_samples.iter().map(|s| s.liq_delta).sum::<f64>()
            / relevant_samples.len() as f64;
        let avg_risk_delta: f64 = relevant_samples.iter().map(|s| s.risk_delta).sum::<f64>()
            / relevant_samples.len() as f64;
        
        // Check if current momentum aligns with historical pattern
        if (avg_liq_delta > 0.0 && liq_mom > 0.0) || (avg_liq_delta < 0.0 && liq_mom < 0.0) {
            score += 0.3;
            reasons.push(format!("Liquidity momentum matches historical {}→{} pattern", from.as_str(), to.as_str()));
        }
        
        if (avg_risk_delta > 0.0 && risk_mom > 0.0) || (avg_risk_delta < 0.0 && risk_mom < 0.0) {
            score += 0.3;
            reasons.push(format!("Risk momentum matches historical {}→{} pattern", from.as_str(), to.as_str()));
        }
        
        // Strong momentum = higher probability
        if liq_mom.abs() > 10.0 || risk_mom.abs() > 10.0 {
            score += 0.2;
            reasons.push("Strong directional momentum detected".to_string());
        }
        
        // Frequency of this transition
        let transition_frequency = relevant_samples.len() as f64 / samples.len() as f64;
        score += transition_frequency * 0.2;
        
        (score.min(1.0), reasons)
    }
    
    /// Heuristic scoring when no historical data exists
    fn heuristic_score(
        &self,
        from: &Regime,
        to: &Regime,
        liq: f64,
        risk: f64,
        unc: f64,
        liq_mom: f64,
        risk_mom: f64,
        vix: Option<f64>,
        fg: Option<f64>,
    ) -> (f64, Vec<String>) {
        let mut score = 0.0;
        let mut reasons = Vec::new();
        
        // CHOPPY → TREND (liquidity improving, risk stable)
        if matches!(from, Regime::Choppy) && matches!(to, Regime::Trend) {
            if liq_mom > 5.0 && risk < 50.0 {
                score = 0.6;
                reasons.push("Liquidity improving in low-risk environment".to_string());
            }
        }
        
        // TREND → PANIC (risk spiking)
        else if matches!(from, Regime::Trend) && matches!(to, Regime::Panic) {
            if risk_mom > 10.0 || (vix.is_some() && vix.unwrap() > 25.0) {
                score = 0.65;
                reasons.push("Risk rapidly increasing".to_string());
            }
        }
        
        // PANIC → CHOPPY (risk cooling down)
        else if matches!(from, Regime::Panic) && matches!(to, Regime::Choppy) {
            if risk_mom < -5.0 {
                score = 0.6;
                reasons.push("Panic subsiding, market stabilizing".to_string());
            }
        }
        
        // Any → GOLD_SUPER_BULL (high liquidity, safe haven demand)
        else if matches!(to, Regime::GoldSuperBull) {
            if liq > 70.0 && risk > 55.0 {
                score = 0.55;
                reasons.push("High liquidity + elevated risk = gold rally setup".to_string());
            }
        }
        
        // Any → BTC_SUPER_BULL (high liquidity, low risk, greed)
        else if matches!(to, Regime::BtcSuperBull) {
            if liq > 70.0 && risk < 40.0 && fg.is_some() && fg.unwrap() > 70.0 {
                score = 0.55;
                reasons.push("High liquidity + low risk + greed = BTC rally".to_string());
            }
        }
        
        (score, reasons)
    }
    
    /// Fetch training samples from database
    async fn fetch_training_samples(&self) -> Result<Vec<TransitionSample>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                r1.regime as from_regime,
                r1.next_regime as to_regime,
                r2.liquidity_score - r1.liquidity_score as liq_delta,
                r2.risk_score - r1.risk_score as risk_delta,
                r2.uncertainty_score - r1.uncertainty_score as unc_delta,
                r1.liquidity_momentum,
                r1.risk_momentum,
                r1.vix_level,
                r1.fear_greed_index,
                EXTRACT(EPOCH FROM (r1.next_regime_timestamp - r1.timestamp)) / 3600.0 as hours_to_transition
            FROM regime_snapshots r1
            JOIN regime_snapshots r2 ON r2.timestamp = r1.next_regime_timestamp
            WHERE r1.next_regime IS NOT NULL
              AND r1.timestamp > NOW() - INTERVAL '30 days'
            ORDER BY r1.timestamp DESC
            LIMIT 500
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let samples = rows
            .into_iter()
            .filter_map(|row| {
                Some(TransitionSample {
                    from_regime: row.from_regime,
                    to_regime: row.to_regime?,
                    liq_delta: row.liq_delta? as f64,
                    risk_delta: row.risk_delta? as f64,
                    unc_delta: row.unc_delta? as f64,
                    liq_momentum: row.liquidity_momentum as f64,
                    risk_momentum: row.risk_momentum as f64,
                    vix_level: row.vix_level.map(|v| v as f64),
                    fear_greed: row.fear_greed_index.map(|f| f as f64),
                    time_to_transition_hours: row.hours_to_transition?.to_string().parse().ok()?,
                })
            })
            .collect();
        
        Ok(samples)
    }
}
