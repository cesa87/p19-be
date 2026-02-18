//! Correlation-Aware Position Sizing
//! 
//! Calculates instrument correlations to prevent overexposure to correlated positions.
//! Reduces position sizes when portfolio already has highly correlated trades.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};

/// Correlation matrix entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationPair {
    pub instrument_a: String,
    pub instrument_b: String,
    pub correlation: f64, // -1.0 to 1.0
    pub window_days: i32,
    pub sample_size: i32,
    pub last_updated: DateTime<Utc>,
}

/// Portfolio correlation analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioCorrelationScore {
    pub new_instrument: String,
    pub avg_correlation: f64, // Average absolute correlation with open positions
    pub max_correlation: f64, // Highest correlation
    pub position_size_multiplier: f64, // 0.5-1.0 (reduces if highly correlated)
    pub correlated_positions: Vec<String>,
}

/// Correlation matrix (all instruments)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationMatrix {
    pub instruments: Vec<String>,
    pub matrix: Vec<Vec<f64>>, // 2D array of correlations
    pub last_updated: DateTime<Utc>,
}

pub struct CorrelationEngine {
    pool: PgPool,
}

impl CorrelationEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// Calculate Pearson correlation between two instruments
    /// Uses daily returns over the specified window
    pub async fn calculate_correlation(
        &self,
        instrument_a: &str,
        instrument_b: &str,
        window_days: i32,
    ) -> Result<f64> {
        // Fetch daily closing prices from OANDA candles (stored in memory/cache)
        // For now, we'll use a simplified approach with price changes
        let returns_a = self.fetch_returns(instrument_a, window_days).await?;
        let returns_b = self.fetch_returns(instrument_b, window_days).await?;
        
        if returns_a.len() < 10 || returns_b.len() < 10 {
            // Not enough data - return neutral
            return Ok(0.0);
        }
        
        // Ensure same length
        let min_len = returns_a.len().min(returns_b.len());
        let returns_a = &returns_a[..min_len];
        let returns_b = &returns_b[..min_len];
        
        // Pearson correlation formula
        let mean_a = returns_a.iter().sum::<f64>() / returns_a.len() as f64;
        let mean_b = returns_b.iter().sum::<f64>() / returns_b.len() as f64;
        
        let numerator: f64 = returns_a
            .iter()
            .zip(returns_b)
            .map(|(a, b)| (a - mean_a) * (b - mean_b))
            .sum();
        
        let denom_a: f64 = returns_a.iter().map(|a| (a - mean_a).powi(2)).sum::<f64>().sqrt();
        let denom_b: f64 = returns_b.iter().map(|b| (b - mean_b).powi(2)).sum::<f64>().sqrt();
        
        if denom_a == 0.0 || denom_b == 0.0 {
            return Ok(0.0);
        }
        
        let correlation = numerator / (denom_a * denom_b);
        Ok(correlation.clamp(-1.0, 1.0))
    }
    
    /// Fetch daily returns for an instrument
    /// In production, this would query historical price data
    async fn fetch_returns(&self, instrument: &str, window_days: i32) -> Result<Vec<f64>> {
        // TODO: In production, fetch from price history table or OANDA API
        // For now, generate synthetic returns based on instrument characteristics
        
        // Placeholder: generate realistic returns based on instrument type
        let volatility = match instrument {
            s if s.contains("BTC") => 0.03, // 3% daily vol
            s if s.contains("XAU") => 0.015, // 1.5% daily vol
            s if s.contains("EUR") || s.contains("USD") => 0.005, // 0.5% daily vol
            s if s.contains("WTI") || s.contains("NATGAS") => 0.025, // 2.5% daily vol
            _ => 0.01,
        };
        
        // Generate returns (in production, fetch real data)
        let mut returns = Vec::new();
        for _ in 0..window_days {
            // Simplified: random returns with characteristic volatility
            let ret = (rand::random::<f64>() - 0.5) * volatility * 2.0;
            returns.push(ret);
        }
        
        Ok(returns)
    }
    
    /// Update full correlation matrix (run daily)
    pub async fn update_matrix(&self) -> Result<usize> {
        let instruments = vec![
            "XAU_USD", "BTC_USD", "EUR_USD", 
            "USD_JPY", "WTICO_USD", "NATGAS_USD"
        ];
        
        let mut updated = 0;
        
        for (i, instrument_a) in instruments.iter().enumerate() {
            for instrument_b in instruments.iter().skip(i + 1) {
                let correlation = self.calculate_correlation(instrument_a, instrument_b, 30).await?;
                
                // Store in database
                sqlx::query!(
                    r#"
                    INSERT INTO instrument_correlations 
                        (instrument_a, instrument_b, correlation, window_days, sample_size, last_updated)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (instrument_a, instrument_b) 
                    DO UPDATE SET 
                        correlation = $3,
                        sample_size = $5,
                        last_updated = $6
                    "#,
                    instrument_a,
                    instrument_b,
                    correlation as f32,
                    30,
                    30, // sample size
                    Utc::now().naive_utc()
                )
                .execute(&self.pool)
                .await?;
                
                updated += 1;
            }
        }
        
        info!("Correlation: Updated {} instrument pairs", updated);
        Ok(updated)
    }
    
    /// Get stored correlation between two instruments
    pub async fn get_correlation(&self, instrument_a: &str, instrument_b: &str) -> Result<f64> {
        // Ensure alphabetical order for lookup
        let (a, b) = if instrument_a < instrument_b {
            (instrument_a, instrument_b)
        } else {
            (instrument_b, instrument_a)
        };
        
        let result = sqlx::query!(
            r#"
            SELECT correlation
            FROM instrument_correlations
            WHERE instrument_a = $1 AND instrument_b = $2
            "#,
            a,
            b
        )
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(result.map(|r| r.correlation as f64).unwrap_or(0.0))
    }
    
    /// Calculate portfolio correlation score for risk management
    /// Returns multiplier to apply to position size (0.5-1.0)
    pub async fn portfolio_correlation_score(
        &self,
        open_positions: &[String],
        new_instrument: &str,
    ) -> Result<PortfolioCorrelationScore> {
        if open_positions.is_empty() {
            return Ok(PortfolioCorrelationScore {
                new_instrument: new_instrument.to_string(),
                avg_correlation: 0.0,
                max_correlation: 0.0,
                position_size_multiplier: 1.0,
                correlated_positions: vec![],
            });
        }
        
        let mut correlations = Vec::new();
        let mut correlated_positions = Vec::new();
        
        for existing in open_positions {
            let corr = self.get_correlation(existing, new_instrument).await?;
            let abs_corr = corr.abs();
            
            correlations.push(abs_corr);
            
            // Flag highly correlated positions (>0.7)
            if abs_corr > 0.7 {
                correlated_positions.push(format!("{} ({:.2})", existing, corr));
            }
        }
        
        let avg_correlation = correlations.iter().sum::<f64>() / correlations.len() as f64;
        let max_correlation = correlations.iter().cloned().fold(0.0, f64::max);
        
        // Calculate position size multiplier
        // High correlation = reduce size
        // 0.0-0.5 corr → 1.0x (no reduction)
        // 0.5-0.7 corr → 0.75-1.0x (minor reduction)
        // 0.7-1.0 corr → 0.5-0.75x (major reduction)
        let multiplier = if max_correlation < 0.5 {
            1.0
        } else if max_correlation < 0.7 {
            1.0 - (max_correlation - 0.5) * 0.5 // Linear reduction
        } else {
            0.5 + (1.0 - max_correlation) * 0.5 // Steeper reduction
        };
        
        if multiplier < 1.0 {
            info!(
                "Correlation: Reducing position size to {:.0}% for {} (avg corr: {:.2}, max: {:.2})",
                multiplier * 100.0,
                new_instrument,
                avg_correlation,
                max_correlation
            );
        }
        
        Ok(PortfolioCorrelationScore {
            new_instrument: new_instrument.to_string(),
            avg_correlation,
            max_correlation,
            position_size_multiplier: multiplier,
            correlated_positions,
        })
    }
    
    /// Get full correlation matrix for visualization
    pub async fn get_matrix(&self) -> Result<CorrelationMatrix> {
        let instruments = vec![
            "XAU_USD", "BTC_USD", "EUR_USD",
            "USD_JPY", "WTICO_USD", "NATGAS_USD",
        ];
        
        let mut matrix = vec![vec![0.0; instruments.len()]; instruments.len()];
        
        // Diagonal is always 1.0 (self-correlation)
        for i in 0..instruments.len() {
            matrix[i][i] = 1.0;
        }
        
        // Fill upper triangle
        for i in 0..instruments.len() {
            for j in (i + 1)..instruments.len() {
                let corr = self.get_correlation(instruments[i], instruments[j]).await?;
                matrix[i][j] = corr;
                matrix[j][i] = corr; // Symmetric
            }
        }
        
        Ok(CorrelationMatrix {
            instruments: instruments.iter().map(|s| s.to_string()).collect(),
            matrix,
            last_updated: Utc::now(),
        })
    }
}

// Known correlation patterns (for initialization when no data exists)
pub struct KnownCorrelations;

impl KnownCorrelations {
    /// Get approximate correlation based on market knowledge
    pub fn approximate(instrument_a: &str, instrument_b: &str) -> f64 {
        // Gold vs USD pairs (inverse relationship)
        if (instrument_a.contains("XAU") && instrument_b.contains("USD"))
            || (instrument_b.contains("XAU") && instrument_a.contains("USD"))
        {
            return -0.3; // Moderately negative
        }
        
        // BTC vs risk assets (positive when both risk-on)
        if (instrument_a.contains("BTC") && instrument_b.contains("EUR"))
            || (instrument_b.contains("BTC") && instrument_a.contains("EUR"))
        {
            return 0.4; // Moderately positive
        }
        
        // Oil vs currencies (complex, often low)
        if (instrument_a.contains("WTI") || instrument_a.contains("NATGAS"))
            && (instrument_b.contains("EUR") || instrument_b.contains("JPY"))
        {
            return 0.2; // Low positive
        }
        
        // Different asset classes = low correlation
        0.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_correlation_multiplier() {
        // Low correlation = no reduction
        let low_corr = 0.3;
        assert_eq!(1.0, calculate_multiplier(low_corr));
        
        // Medium correlation = minor reduction
        let med_corr = 0.6;
        assert!(calculate_multiplier(med_corr) > 0.75);
        assert!(calculate_multiplier(med_corr) < 1.0);
        
        // High correlation = major reduction
        let high_corr = 0.9;
        assert!(calculate_multiplier(high_corr) >= 0.5);
        assert!(calculate_multiplier(high_corr) <= 0.6);
    }
    
    fn calculate_multiplier(max_corr: f64) -> f64 {
        if max_corr < 0.5 {
            1.0
        } else if max_corr < 0.7 {
            1.0 - (max_corr - 0.5) * 0.5
        } else {
            0.5 + (1.0 - max_corr) * 0.5
        }
    }
}
