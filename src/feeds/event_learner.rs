//! News-Event Impact Learning
//! 
//! Learns which economic calendar events actually move markets.
//! Analyzes post-event volatility to build impact scores (0-10).
//! Only pauses trading for truly high-impact events.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};

/// Learned event impact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventImpact {
    pub event_name: String,
    pub event_category: Option<String>,
    pub avg_volatility_spike: f64, // Average ATR spike (percentage)
    pub max_volatility_spike: f64,
    pub avg_price_move_pct: f64, // Average absolute price move
    pub sample_size: i32,
    pub impact_score: i32, // 0-10 scale
    pub last_occurrence: Option<DateTime<Utc>>,
}

impl EventImpact {
    /// Determine if this is a high-impact event that should pause trading
    pub fn should_pause_trading(&self) -> bool {
        self.impact_score >= 7
    }
    
    /// Get impact description
    pub fn impact_description(&self) -> &'static str {
        match self.impact_score {
            0..=2 => "Negligible",
            3..=4 => "Low",
            5..=6 => "Medium",
            7..=8 => "High",
            9..=10 => "Extreme",
            _ => "Unknown",
        }
    }
}

pub struct EventLearner {
    pool: PgPool,
}

impl EventLearner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Analyze volatility and price impact after an event
    /// Call this 30 minutes after an event occurs
    pub async fn analyze_event_impact(
        &self,
        event_name: &str,
        event_time: DateTime<Utc>,
        primary_instrument: &str,
    ) -> Result<EventImpact> {
        // Get volatility before and after event
        let before_window = event_time - Duration::minutes(30);
        let after_window = event_time + Duration::minutes(30);
        
        // Calculate ATR (volatility) before event
        let vol_before = self.estimate_volatility_window(primary_instrument, before_window, 30).await?;
        
        // Calculate ATR after event
        let vol_after = self.estimate_volatility_window(primary_instrument, after_window, 30).await?;
        
        let vol_spike = if vol_before > 0.0 {
            ((vol_after - vol_before) / vol_before) * 100.0
        } else {
            0.0
        };
        
        // Calculate price move
        let price_before = self.estimate_price_at_time(primary_instrument, event_time).await?;
        let price_after = self.estimate_price_at_time(primary_instrument, after_window).await?;
        
        let price_move_pct = if price_before > 0.0 {
            ((price_after - price_before).abs() / price_before) * 100.0
        } else {
            0.0
        };
        
        // Calculate impact score (0-10)
        let impact_score = self.calculate_impact_score(vol_spike, price_move_pct);
        
        // Update database with running average
        self.update_event_stats(event_name, vol_spike, price_move_pct, impact_score).await?;
        
        info!(
            "Event learner: {} - vol spike: {:.1}%, price move: {:.2}%, impact: {}/10",
            event_name, vol_spike, price_move_pct, impact_score
        );
        
        // Fetch updated stats
        self.get_event_impact(event_name).await
    }
    
    /// Calculate impact score from volatility and price movement
    fn calculate_impact_score(&self, vol_spike_pct: f64, price_move_pct: f64) -> i32 {
        // Weight: volatility spike 40%, price move 60%
        let vol_component = (vol_spike_pct.clamp(0.0, 200.0) / 200.0) * 4.0; // 0-4 points
        let price_component = (price_move_pct.clamp(0.0, 2.0) / 2.0) * 6.0; // 0-6 points
        
        let score = (vol_component + price_component).round() as i32;
        score.clamp(0, 10)
    }
    
    /// Estimate volatility in a time window (simplified)
    async fn estimate_volatility_window(
        &self,
        _instrument: &str,
        _time: DateTime<Utc>,
        _minutes: i32,
    ) -> Result<f64> {
        // TODO: In production, fetch actual candle data from OANDA
        // Calculate ATR from high-low ranges
        
        // Placeholder: return realistic volatility
        Ok(0.15) // 0.15% typical volatility
    }
    
    /// Estimate price at specific time (simplified)
    async fn estimate_price_at_time(
        &self,
        _instrument: &str,
        _time: DateTime<Utc>,
    ) -> Result<f64> {
        // TODO: In production, fetch actual price from OANDA
        
        // Placeholder: return realistic price
        Ok(2850.0) // e.g. Gold price
    }
    
    /// Update event statistics with new observation
    async fn update_event_stats(
        &self,
        event_name: &str,
        vol_spike: f64,
        price_move: f64,
        impact_score: i32,
    ) -> Result<()> {
        // Fetch existing stats
        let existing = sqlx::query!(
            r#"
            SELECT avg_volatility_spike, max_volatility_spike, avg_price_move_pct, sample_size
            FROM event_impacts
            WHERE event_name = $1
            "#,
            event_name
        )
        .fetch_optional(&self.pool)
        .await?;
        
        let (new_avg_vol, new_max_vol, new_avg_price, new_sample_size) = if let Some(existing) = existing {
            // Update running average
            let n = existing.sample_size as f64;
            let new_avg_vol = ((existing.avg_volatility_spike as f64 * n) + vol_spike) / (n + 1.0);
            let new_max_vol = existing.max_volatility_spike.unwrap_or(0.0).max(vol_spike as f32);
            let new_avg_price = ((existing.avg_price_move_pct.unwrap_or(0.0) as f64 * n) + price_move) / (n + 1.0);
            let new_sample_size = existing.sample_size + 1;
            
            (new_avg_vol, new_max_vol, new_avg_price, new_sample_size)
        } else {
            // First observation
            (vol_spike, vol_spike as f32, price_move, 1)
        };
        
        // Insert or update
        sqlx::query!(
            r#"
            INSERT INTO event_impacts 
                (event_name, avg_volatility_spike, max_volatility_spike, 
                 avg_price_move_pct, sample_size, impact_score, last_occurrence, last_updated)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_name)
            DO UPDATE SET
                avg_volatility_spike = $2,
                max_volatility_spike = $3,
                avg_price_move_pct = $4,
                sample_size = $5,
                impact_score = $6,
                last_occurrence = $7,
                last_updated = $8
            "#,
            event_name,
            new_avg_vol as f32,
            new_max_vol as f32,
            new_avg_price as f32,
            new_sample_size,
            impact_score,
            Utc::now().naive_utc(),
            Utc::now().naive_utc()
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Get learned impact for an event
    pub async fn get_event_impact(&self, event_name: &str) -> Result<EventImpact> {
        let result = sqlx::query!(
            r#"
            SELECT event_name, event_category, avg_volatility_spike, max_volatility_spike,
                   avg_price_move_pct, sample_size, impact_score, last_occurrence
            FROM event_impacts
            WHERE event_name = $1
            "#,
            event_name
        )
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(row) = result {
            Ok(EventImpact {
                event_name: row.event_name,
                event_category: row.event_category,
                avg_volatility_spike: row.avg_volatility_spike as f64,
                max_volatility_spike: row.max_volatility_spike.unwrap_or(0.0) as f64,
                avg_price_move_pct: row.avg_price_move_pct.unwrap_or(0.0) as f64,
                sample_size: row.sample_size,
                impact_score: row.impact_score,
                last_occurrence: row.last_occurrence.map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc)),
            })
        } else {
            // Unknown event - return default medium impact
            Ok(EventImpact {
                event_name: event_name.to_string(),
                event_category: None,
                avg_volatility_spike: 0.0,
                max_volatility_spike: 0.0,
                avg_price_move_pct: 0.0,
                sample_size: 0,
                impact_score: 5, // Default to medium until we learn
                last_occurrence: None,
            })
        }
    }
    
    /// Get all events sorted by impact score
    pub async fn get_all_events(&self) -> Result<Vec<EventImpact>> {
        let rows = sqlx::query!(
            r#"
            SELECT event_name, event_category, avg_volatility_spike, max_volatility_spike,
                   avg_price_move_pct, sample_size, impact_score, last_occurrence
            FROM event_impacts
            ORDER BY impact_score DESC, sample_size DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows
            .into_iter()
            .map(|row| EventImpact {
                event_name: row.event_name,
                event_category: row.event_category,
                avg_volatility_spike: row.avg_volatility_spike as f64,
                max_volatility_spike: row.max_volatility_spike.unwrap_or(0.0) as f64,
                avg_price_move_pct: row.avg_price_move_pct.unwrap_or(0.0) as f64,
                sample_size: row.sample_size,
                impact_score: row.impact_score,
                last_occurrence: row.last_occurrence.map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc)),
            })
            .collect())
    }
    
    /// Get high-impact events (score >= 7)
    pub async fn get_high_impact_events(&self) -> Result<Vec<EventImpact>> {
        let all = self.get_all_events().await?;
        Ok(all.into_iter().filter(|e| e.impact_score >= 7).collect())
    }
    
    /// Categorize event by name patterns
    pub fn categorize_event(event_name: &str) -> String {
        let name_lower = event_name.to_lowercase();
        
        if name_lower.contains("nfp") || name_lower.contains("employment") || name_lower.contains("jobless") {
            "Employment".to_string()
        } else if name_lower.contains("cpi") || name_lower.contains("inflation") || name_lower.contains("ppi") {
            "Inflation".to_string()
        } else if name_lower.contains("fomc") || name_lower.contains("fed") || name_lower.contains("interest rate") {
            "Central Bank".to_string()
        } else if name_lower.contains("gdp") || name_lower.contains("retail") || name_lower.contains("manufacturing") {
            "Growth".to_string()
        } else if name_lower.contains("trade") || name_lower.contains("balance") {
            "Trade".to_string()
        } else {
            "Other".to_string()
        }
    }
}

/// Known high-impact events (for initialization)
pub struct KnownHighImpactEvents;

impl KnownHighImpactEvents {
    pub fn list() -> Vec<(&'static str, i32)> {
        vec![
            ("Non-Farm Payrolls", 9),
            ("FOMC Rate Decision", 9),
            ("CPI", 8),
            ("GDP", 7),
            ("Unemployment Rate", 7),
            ("Retail Sales", 6),
            ("PPI", 6),
            ("ISM Manufacturing", 5),
            ("Consumer Confidence", 4),
            ("Housing Starts", 4),
            ("Trade Balance", 3),
            ("Initial Jobless Claims", 3),
        ]
    }
    
    /// Initialize database with known events
    pub async fn initialize(pool: &PgPool) -> Result<usize> {
        let mut count = 0;
        
        for (event_name, score) in Self::list() {
            sqlx::query!(
                r#"
                INSERT INTO event_impacts 
                    (event_name, event_category, avg_volatility_spike, avg_price_move_pct, 
                     sample_size, impact_score, last_updated)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (event_name) DO NOTHING
                "#,
                event_name,
                EventLearner::categorize_event(event_name),
                0.0_f32, // Will be learned
                0.0_f32, // Will be learned
                0_i32,   // No samples yet
                score,
                Utc::now().naive_utc()
            )
            .execute(pool)
            .await?;
            
            count += 1;
        }
        
        info!("Event learner: Initialized {} known events", count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_impact_score_calculation() {
        let learner = EventLearner { pool: todo!() };
        
        // High volatility + high price move = high score
        assert_eq!(10, learner.calculate_impact_score(200.0, 2.0));
        
        // Medium impact
        assert_eq!(5, learner.calculate_impact_score(50.0, 0.5));
        
        // Low impact
        assert_eq!(1, learner.calculate_impact_score(10.0, 0.1));
        
        // No impact
        assert_eq!(0, learner.calculate_impact_score(0.0, 0.0));
    }
    
    #[test]
    fn test_event_categorization() {
        assert_eq!("Employment", EventLearner::categorize_event("Non-Farm Payrolls"));
        assert_eq!("Inflation", EventLearner::categorize_event("CPI Report"));
        assert_eq!("Central Bank", EventLearner::categorize_event("FOMC Rate Decision"));
        assert_eq!("Growth", EventLearner::categorize_event("GDP Growth"));
        assert_eq!("Other", EventLearner::categorize_event("Random Event"));
    }
    
    #[test]
    fn test_should_pause_trading() {
        let high_impact = EventImpact {
            event_name: "NFP".to_string(),
            event_category: Some("Employment".to_string()),
            avg_volatility_spike: 100.0,
            max_volatility_spike: 150.0,
            avg_price_move_pct: 1.5,
            sample_size: 10,
            impact_score: 9,
            last_occurrence: None,
        };
        
        let low_impact = EventImpact {
            event_name: "Minor Event".to_string(),
            event_category: None,
            avg_volatility_spike: 10.0,
            max_volatility_spike: 15.0,
            avg_price_move_pct: 0.1,
            sample_size: 5,
            impact_score: 3,
            last_occurrence: None,
        };
        
        assert!(high_impact.should_pause_trading());
        assert!(!low_impact.should_pause_trading());
    }
}
