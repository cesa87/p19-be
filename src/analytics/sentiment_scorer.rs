//! Sentiment-Driven Confidence Scorer
//! 
//! Applies contrarian logic to Fear & Greed + Reddit sentiment.
//! Boosts confidence for contrarian trades, reduces for consensus trades.

use serde::{Deserialize, Serialize};

/// Sentiment signal with confidence adjustment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentSignal {
    pub multiplier: f64, // 0.85-1.2
    pub signal_type: String, // "extreme_fear", "extreme_greed", "neutral", "conflict"
    pub reason: String,
    pub fear_greed_level: Option<f64>,
    pub reddit_sentiment: Option<f64>,
}

impl Default for SentimentSignal {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            signal_type: "neutral".to_string(),
            reason: "No strong sentiment signal".to_string(),
            fear_greed_level: None,
            reddit_sentiment: None,
        }
    }
}

pub struct SentimentScorer;

impl SentimentScorer {
    /// Calculate contrarian confidence multiplier
    /// 
    /// # Arguments
    /// * `fear_greed` - Fear & Greed Index (0-100, lower = more fear)
    /// * `reddit_crypto` - Reddit crypto sentiment (-1.0 to 1.0)
    /// * `reddit_btc` - Reddit BTC sentiment (-1.0 to 1.0)
    /// * `reddit_gold` - Reddit gold sentiment (-1.0 to 1.0)
    /// * `bot_direction` - "LONG" or "SHORT"
    /// * `instrument` - Trading instrument (e.g. "XAU_USD", "BTC_USD")
    pub fn score_sentiment(
        fear_greed: Option<f64>,
        reddit_crypto: Option<f64>,
        reddit_btc: Option<f64>,
        reddit_gold: Option<f64>,
        bot_direction: &str,
        instrument: &str,
    ) -> SentimentSignal {
        let fg = fear_greed.unwrap_or(50.0);
        
        let mut multiplier: f64 = 1.0;
        let mut signal_type = "neutral";
        let mut reasons = Vec::new();
        
        // ═══════════════════════════════════════════════════════════════════
        // CONTRARIAN SIGNALS (Extreme Fear/Greed)
        // ═══════════════════════════════════════════════════════════════════
        
        // EXTREME FEAR (< 20) = Contrarian BUY opportunity
        if fg < 20.0 && bot_direction == "LONG" {
            multiplier *= 1.2;
            signal_type = "extreme_fear";
            reasons.push(format!("⚡ Extreme fear ({:.0}/100) - contrarian buy signal", fg));
        }
        // EXTREME GREED (> 80) = Contrarian SELL opportunity
        else if fg > 80.0 && bot_direction == "SHORT" {
            multiplier *= 1.2;
            signal_type = "extreme_greed";
            reasons.push(format!("⚡ Extreme greed ({:.0}/100) - contrarian sell signal", fg));
        }
        // CONFLICT: Sentiment opposes trade direction
        else if (fg < 30.0 && bot_direction == "SHORT") || (fg > 70.0 && bot_direction == "LONG") {
            multiplier *= 0.85;
            signal_type = "conflict";
            if fg < 30.0 {
                reasons.push(format!("⚠️ Fear ({:.0}/100) conflicts with SHORT - risky fade", fg));
            } else {
                reasons.push(format!("⚠️ Greed ({:.0}/100) conflicts with LONG - risky chase", fg));
            }
        }
        
        // ═══════════════════════════════════════════════════════════════════
        // REDDIT CONFIRMATION
        // ═══════════════════════════════════════════════════════════════════
        
        // Bitcoin-specific Reddit sentiment
        if instrument.contains("BTC") {
            if let Some(reddit_btc_sent) = reddit_btc {
                // Strong bullish Reddit + LONG = confirmation boost
                if reddit_btc_sent > 0.5 && bot_direction == "LONG" {
                    multiplier *= 1.1;
                    reasons.push(format!("✅ Reddit BTC bullish (+{:.0}%) confirms LONG", reddit_btc_sent * 100.0));
                }
                // Strong bearish Reddit + SHORT = confirmation boost
                else if reddit_btc_sent < -0.3 && bot_direction == "SHORT" {
                    multiplier *= 1.1;
                    reasons.push(format!("✅ Reddit BTC bearish ({:.0}%) confirms SHORT", reddit_btc_sent * 100.0));
                }
                // Conflict
                else if (reddit_btc_sent > 0.3 && bot_direction == "SHORT") 
                     || (reddit_btc_sent < -0.2 && bot_direction == "LONG") {
                    multiplier *= 0.9;
                    reasons.push("⚠️ Reddit BTC sentiment conflicts with direction".to_string());
                }
            }
        }
        
        // Gold-specific Reddit sentiment
        if instrument.contains("XAU") || instrument.contains("GOLD") {
            if let Some(reddit_gold_sent) = reddit_gold {
                if reddit_gold_sent > 0.3 && bot_direction == "LONG" {
                    multiplier *= 1.1;
                    reasons.push(format!("✅ Reddit gold bullish (+{:.0}%) confirms LONG", reddit_gold_sent * 100.0));
                }
                else if reddit_gold_sent < -0.2 && bot_direction == "SHORT" {
                    multiplier *= 1.1;
                    reasons.push(format!("✅ Reddit gold bearish ({:.0}%) confirms SHORT", reddit_gold_sent * 100.0));
                }
            }
        }
        
        // General crypto sentiment (for any crypto instrument)
        if instrument.contains("BTC") || instrument.contains("ETH") || instrument.contains("CRYPTO") {
            if let Some(reddit_crypto_sent) = reddit_crypto {
                // Extreme negative crypto sentiment + LONG crypto = contrarian
                if reddit_crypto_sent < -0.4 && bot_direction == "LONG" {
                    multiplier *= 1.15;
                    signal_type = "extreme_fear";
                    reasons.push(format!("⚡ Reddit crypto panic ({:.0}%) - contrarian buy", reddit_crypto_sent * 100.0));
                }
                // Extreme positive crypto sentiment + SHORT crypto = contrarian
                else if reddit_crypto_sent > 0.6 && bot_direction == "SHORT" {
                    multiplier *= 1.15;
                    signal_type = "extreme_greed";
                    reasons.push(format!("⚡ Reddit crypto euphoria (+{:.0}%) - contrarian sell", reddit_crypto_sent * 100.0));
                }
            }
        }
        
        // ═══════════════════════════════════════════════════════════════════
        // SAFE HAVEN ROTATION (Gold vs Risk Assets)
        // ═══════════════════════════════════════════════════════════════════
        
        // Fear + Gold LONG = strong confluence
        if fg < 40.0 && instrument.contains("XAU") && bot_direction == "LONG" {
            multiplier *= 1.05;
            reasons.push("✅ Fear drives safe-haven demand for gold".to_string());
        }
        // Greed + Gold SHORT = risk-on rotation away from gold
        else if fg > 60.0 && instrument.contains("XAU") && bot_direction == "SHORT" {
            multiplier *= 1.05;
            reasons.push("✅ Greed drives rotation out of safe havens".to_string());
        }
        
        // Clamp multiplier to safe range
        multiplier = multiplier.clamp(0.85, 1.25);
        
        let reason = if reasons.is_empty() {
            format!("Neutral sentiment (F&G: {:.0}/100)", fg)
        } else {
            reasons.join(" • ")
        };
        
        SentimentSignal {
            multiplier,
            signal_type: signal_type.to_string(),
            reason,
            fear_greed_level: fear_greed,
            reddit_sentiment: reddit_crypto.or(reddit_btc).or(reddit_gold),
        }
    }
    
    /// Determine overall market sentiment category
    pub fn categorize_sentiment(fear_greed: Option<f64>) -> &'static str {
        let fg = fear_greed.unwrap_or(50.0);
        
        if fg < 20.0 {
            "extreme_fear"
        } else if fg < 40.0 {
            "fear"
        } else if fg < 60.0 {
            "neutral"
        } else if fg < 80.0 {
            "greed"
        } else {
            "extreme_greed"
        }
    }
    
    /// Check if sentiment is at an extreme (good for contrarian trades)
    pub fn is_extreme(fear_greed: Option<f64>) -> bool {
        if let Some(fg) = fear_greed {
            fg < 20.0 || fg > 80.0
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extreme_fear_long() {
        let signal = SentimentScorer::score_sentiment(
            Some(15.0), // Extreme fear
            None,
            None,
            None,
            "LONG",
            "XAU_USD",
        );
        
        assert!(signal.multiplier > 1.0, "Should boost LONG confidence in extreme fear");
        assert_eq!(signal.signal_type, "extreme_fear");
    }
    
    #[test]
    fn test_extreme_greed_short() {
        let signal = SentimentScorer::score_sentiment(
            Some(85.0), // Extreme greed
            None,
            None,
            None,
            "SHORT",
            "BTC_USD",
        );
        
        assert!(signal.multiplier > 1.0, "Should boost SHORT confidence in extreme greed");
        assert_eq!(signal.signal_type, "extreme_greed");
    }
    
    #[test]
    fn test_sentiment_conflict() {
        let signal = SentimentScorer::score_sentiment(
            Some(25.0), // Fear
            None,
            None,
            None,
            "SHORT", // Trying to short during fear
            "XAU_USD",
        );
        
        assert!(signal.multiplier < 1.0, "Should reduce confidence when sentiment conflicts");
        assert_eq!(signal.signal_type, "conflict");
    }
    
    #[test]
    fn test_reddit_confirmation() {
        let signal = SentimentScorer::score_sentiment(
            Some(50.0),
            None,
            Some(0.7), // Strong bullish BTC sentiment
            None,
            "LONG",
            "BTC_USD",
        );
        
        assert!(signal.multiplier > 1.0, "Should boost when Reddit confirms direction");
    }
}
