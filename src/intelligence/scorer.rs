//! Intelligence Scorer — aggregates feed snapshots into per-instrument scores

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::info;

use super::aggregator::FeedSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopSignal {
    pub feed_id: String,
    pub description: String,
    pub impact: f64,  // -1.0 to 1.0, negative = bearish
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceScore {
    pub instrument: String,
    pub tension_score: f64,      // 0-100 macro/geopolitical risk
    pub opportunity_score: f64,  // 0-100 trade signal strength
    pub direction_bias: Option<String>,  // "LONG", "SHORT", or None
    pub confidence: f64,         // 0-1
    pub top_signals: Vec<TopSignal>,
    pub created_at: DateTime<Utc>,
}

pub struct IntelligenceScorer {
    pool: PgPool,
}

impl IntelligenceScorer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn calculate_scores(&self, snapshots: &[FeedSnapshot]) -> Result<Vec<IntelligenceScore>> {
        let instruments = ["XAU_USD", "EUR_USD", "BTC_USD", "USD_JPY", "NATGAS_USD", "WTICO_USD"];
        let mut scores = Vec::new();

        for instrument in &instruments {
            let score = self.score_instrument(instrument, snapshots);
            self.store_score(&score).await?;
            scores.push(score);
        }

        info!("🧠 Scores calculated for {} instruments", scores.len());
        Ok(scores)
    }

    fn score_instrument(&self, instrument: &str, snapshots: &[FeedSnapshot]) -> IntelligenceScore {
        let mut tension_inputs: Vec<(f64, f64)> = Vec::new(); // (value, weight)
        let mut opportunity_inputs: Vec<(f64, f64)> = Vec::new();
        let mut top_signals: Vec<TopSignal> = Vec::new();
        let mut bullish_votes = 0.0f64;
        let mut bearish_votes = 0.0f64;
        let mut total_votes = 0.0f64;

        for snap in snapshots {
            match snap.feed_id.as_str() {
                // VIX: high fear = tension, low fear = opportunity
                "vix" => {
                    tension_inputs.push((snap.value, 2.0));
                    if snap.value > 70.0 {
                        top_signals.push(TopSignal {
                            feed_id: snap.feed_id.clone(),
                            description: format!("High fear: {}", snap.label),
                            impact: -0.8,
                        });
                    }
                }
                // Fear & Greed: invert for tension (extreme fear = high tension)
                "fear_greed_stocks" | "fear_greed_crypto" => {
                    let tension = 100.0 - snap.value; // invert
                    tension_inputs.push((tension, 1.5));
                    // Extreme greed = opportunity
                    if snap.value > 75.0 {
                        opportunity_inputs.push((snap.value, 1.0));
                    }
                }
                // DXY: high DXY = bearish Gold pressure
                "dxy" => {
                    if instrument == "XAU_USD" {
                        let gold_pressure = snap.value; // already normalised for gold pressure
                        tension_inputs.push((gold_pressure, 1.5));
                        if snap.value > 70.0 {
                            bearish_votes += 1.5;
                            total_votes += 1.5;
                            top_signals.push(TopSignal {
                                feed_id: snap.feed_id.clone(),
                                description: format!("Strong DXY: bearish Gold — {}", snap.label),
                                impact: -0.6,
                            });
                        } else if snap.value < 30.0 {
                            bullish_votes += 1.5;
                            total_votes += 1.5;
                        }
                    }
                }
                // Real yield: high real yield = bearish Gold (already converted to gold score)
                "real_yield_10y" => {
                    if instrument == "XAU_USD" {
                        opportunity_inputs.push((snap.value, 2.0));
                        if snap.value > 60.0 {
                            bullish_votes += 2.0;
                            total_votes += 2.0;
                        } else if snap.value < 40.0 {
                            bearish_votes += 2.0;
                            total_votes += 2.0;
                        }
                    }
                }
                // Inverted yield curve: tension signal
                "yield_curve_slope" => {
                    tension_inputs.push((snap.value, 1.5));
                    if snap.value > 70.0 {
                        top_signals.push(TopSignal {
                            feed_id: snap.feed_id.clone(),
                            description: format!("Yield curve warning — {}", snap.label),
                            impact: -0.5,
                        });
                    }
                }
                // Reddit sentiment: bullish = opportunity
                "reddit_sentiment" => {
                    opportunity_inputs.push((snap.value, 1.0));
                    if snap.value > 65.0 {
                        bullish_votes += 1.0;
                        total_votes += 1.0;
                    } else if snap.value < 35.0 {
                        bearish_votes += 1.0;
                        total_votes += 1.0;
                    }
                }
                // GLD flows: positive flow = bullish gold
                "gld_flow" => {
                    if instrument == "XAU_USD" {
                        opportunity_inputs.push((snap.value, 1.5));
                        if snap.value > 60.0 {
                            bullish_votes += 1.5;
                            total_votes += 1.5;
                        } else if snap.value < 40.0 {
                            bearish_votes += 1.5;
                            total_votes += 1.5;
                        }
                    }
                }
                // Polymarket
                "polymarket_fed" => {
                    tension_inputs.push((snap.value, 1.5));
                    if snap.value > 70.0 {
                        top_signals.push(TopSignal {
                            feed_id: snap.feed_id.clone(),
                            description: format!("High Fed policy uncertainty — {}", snap.label),
                            impact: -0.4,
                        });
                    }
                }
                "polymarket_btc" => {
                    if instrument == "BTC_USD" {
                        opportunity_inputs.push((snap.value, 2.0));
                        if snap.value > 60.0 { bullish_votes += 2.0; total_votes += 2.0; }
                        else if snap.value < 40.0 { bearish_votes += 2.0; total_votes += 2.0; }
                    }
                }
                "polymarket_risk" => {
                    tension_inputs.push((snap.value, 2.0));
                    if snap.value > 70.0 {
                        top_signals.push(TopSignal {
                            feed_id: snap.feed_id.clone(),
                            description: format!("Polymarket elevated risk — {}", snap.label),
                            impact: -0.7,
                        });
                    }
                }
                // News
                "alphavantage_news" | "reddit_sentiment" => {
                    opportunity_inputs.push((snap.value, 1.0));
                }
                "newsapi_uncertainty" => {
                    tension_inputs.push((snap.value, 1.5));
                }
                "finnhub_events" => {
                    tension_inputs.push((snap.value, 1.0));
                    if snap.value > 60.0 {
                        top_signals.push(TopSignal {
                            feed_id: snap.feed_id.clone(),
                            description: format!("High-impact economic events — {}", snap.label),
                            impact: -0.3,
                        });
                    }
                }
                _ => {}
            }
        }

        // Weighted average
        let tension_score = weighted_avg(&tension_inputs).unwrap_or(50.0);
        let opportunity_score = weighted_avg(&opportunity_inputs).unwrap_or(50.0);

        // Direction bias
        let direction_bias = if total_votes > 0.0 {
            let bull_pct = bullish_votes / total_votes;
            if bull_pct > 0.65 { Some("LONG".to_string()) }
            else if bull_pct < 0.35 { Some("SHORT".to_string()) }
            else { None }
        } else {
            None
        };

        // Confidence: based on how many feeds contributed and how aligned they are
        let feed_count = (tension_inputs.len() + opportunity_inputs.len()) as f64;
        let alignment = if total_votes > 0.0 {
            ((bullish_votes - bearish_votes).abs() / total_votes)
        } else { 0.0 };
        let confidence = ((feed_count / 12.0) * 0.6 + alignment * 0.4).clamp(0.0, 1.0);

        // Keep top 5 signals by abs impact
        top_signals.sort_by(|a, b| b.impact.abs().partial_cmp(&a.impact.abs()).unwrap());
        top_signals.truncate(5);

        IntelligenceScore {
            instrument: instrument.to_string(),
            tension_score,
            opportunity_score,
            direction_bias,
            confidence,
            top_signals,
            created_at: Utc::now(),
        }
    }

    async fn store_score(&self, score: &IntelligenceScore) -> Result<()> {
        let top_signals = serde_json::to_string(&score.top_signals)?;
        sqlx::query(
            "INSERT INTO intelligence_scores (instrument, tension_score, opportunity_score, direction_bias, confidence, top_signals, created_at)
             VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7)"
        )
        .bind(&score.instrument)
        .bind(score.tension_score)
        .bind(score.opportunity_score)
        .bind(&score.direction_bias)
        .bind(score.confidence)
        .bind(&top_signals)
        .bind(score.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_latest_scores(&self) -> Result<Vec<IntelligenceScore>> {
        let rows = sqlx::query!(
            r#"SELECT DISTINCT ON (instrument) instrument, tension_score, opportunity_score,
               direction_bias, confidence, top_signals, created_at
               FROM intelligence_scores
               ORDER BY instrument, created_at DESC"#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| IntelligenceScore {
            instrument: r.instrument,
            tension_score: r.tension_score,
            opportunity_score: r.opportunity_score,
            direction_bias: r.direction_bias,
            confidence: r.confidence,
            top_signals: r.top_signals.as_ref()
                .and_then(|j| serde_json::from_value(j.clone()).ok())
                .unwrap_or_default(),
            created_at: r.created_at,
        }).collect())
    }

    pub async fn get_score_history(&self, instrument: &str, hours: i64) -> Result<Vec<IntelligenceScore>> {
        let since = Utc::now() - chrono::Duration::hours(hours);
        let rows = sqlx::query!(
            r#"SELECT instrument, tension_score, opportunity_score, direction_bias, confidence, top_signals, created_at
               FROM intelligence_scores
               WHERE instrument = $1 AND created_at > $2
               ORDER BY created_at ASC"#,
            instrument, since
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| IntelligenceScore {
            instrument: r.instrument,
            tension_score: r.tension_score,
            opportunity_score: r.opportunity_score,
            direction_bias: r.direction_bias,
            confidence: r.confidence,
            top_signals: r.top_signals.as_ref()
                .and_then(|j| serde_json::from_value(j.clone()).ok())
                .unwrap_or_default(),
            created_at: r.created_at,
        }).collect())
    }
}

fn weighted_avg(inputs: &[(f64, f64)]) -> Option<f64> {
    if inputs.is_empty() { return None; }
    let total_weight: f64 = inputs.iter().map(|(_, w)| w).sum();
    if total_weight == 0.0 { return None; }
    Some(inputs.iter().map(|(v, w)| v * w).sum::<f64>() / total_weight)
}
