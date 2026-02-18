use regex::Regex;
use serde::{Deserialize, Serialize};

/// Parsed trade signal from a Telegram message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    pub direction: TradeDirection,
    pub symbol: String,
    pub entry_price: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Vec<f64>,  // Can have multiple TPs
    pub lot_size: Option<f64>,
    pub raw_message: String,
    pub confidence: f64,  // 0.0 - 1.0, how confident we are in the parse
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeDirection {
    Buy,
    Sell,
}

impl std::fmt::Display for TradeDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeDirection::Buy => write!(f, "BUY"),
            TradeDirection::Sell => write!(f, "SELL"),
        }
    }
}

/// Parser for extracting trade signals from text messages
pub struct SignalParser {
    // Regex patterns for common signal formats
    direction_pattern: Regex,
    price_pattern: Regex,
    sl_pattern: Regex,
    tp_pattern: Regex,
    gold_pattern: Regex,
}

impl SignalParser {
    pub fn new() -> Self {
        Self {
            // Match BUY/SELL/LONG/SHORT
            direction_pattern: Regex::new(r"(?i)\b(buy|sell|long|short)\b").unwrap(),
            
            // Match prices like 2650, 2650.50, @2650, @ 2650, @2650-2660 (range - take first)
            price_pattern: Regex::new(r"@\s*(\d{4,5}(?:\.\d{1,2})?)").unwrap(),
            
            // Match SL patterns: SL 2640, SL: 2640, Sl :2640, stop loss 2640, sl@2640
            sl_pattern: Regex::new(r"(?i)(?:sl|stop\s*loss|stop)\s*[:\s@]\s*(\d{4,5}(?:\.\d{1,2})?)").unwrap(),
            
            // Match TP patterns: TP 2660, TP1 2660, Tp1 : 2660, take profit 2660
            tp_pattern: Regex::new(r"(?i)(?:tp\d?|take\s*profit\d?)\s*[:\s@]\s*(\d{4,5}(?:\.\d{1,2})?)").unwrap(),
            
            // Match gold references: XAUUSD, XAU/USD, GOLD, gold
            gold_pattern: Regex::new(r"(?i)\b(xau/?usd|gold)\b").unwrap(),
        }
    }

    /// Parse a message and attempt to extract a trade signal
    pub fn parse(&self, message: &str) -> Option<TradeSignal> {
        let message_upper = message.to_uppercase();
        
        // Must have a direction
        let direction = self.extract_direction(&message_upper)?;
        
        // Check if it's about gold (for now we only support gold)
        let symbol = if self.gold_pattern.is_match(message) {
            "XAU_USD".to_string()
        } else {
            // Could be implied gold context, or skip
            "XAU_USD".to_string()  // Default to gold for now
        };
        
        // Extract entry price (first price that's not SL/TP)
        let entry_price = self.extract_entry_price(message);
        
        // Extract stop loss
        let stop_loss = self.extract_stop_loss(message);
        
        // Extract take profit(s)
        let take_profit = self.extract_take_profits(message);
        
        // Calculate confidence based on how much info we extracted
        let confidence = self.calculate_confidence(&direction, &entry_price, &stop_loss, &take_profit);
        
        // Only return if we have minimum confidence
        if confidence < 0.3 {
            return None;
        }
        
        Some(TradeSignal {
            direction,
            symbol,
            entry_price,
            stop_loss,
            take_profit,
            lot_size: None,  // Usually not specified in signals
            raw_message: message.to_string(),
            confidence,
        })
    }

    fn extract_direction(&self, message: &str) -> Option<TradeDirection> {
        if let Some(caps) = self.direction_pattern.captures(message) {
            let dir = caps.get(1)?.as_str().to_uppercase();
            match dir.as_str() {
                "BUY" | "LONG" => Some(TradeDirection::Buy),
                "SELL" | "SHORT" => Some(TradeDirection::Sell),
                _ => None,
            }
        } else {
            None
        }
    }

    fn extract_entry_price(&self, message: &str) -> Option<f64> {
        // Remove SL and TP sections to find entry price
        let cleaned = self.sl_pattern.replace_all(message, "");
        let cleaned = self.tp_pattern.replace_all(&cleaned, "");
        
        // Find first price-like number (gold prices are typically 1800-6000+)
        for caps in self.price_pattern.captures_iter(&cleaned) {
            if let Some(price_str) = caps.get(1) {
                if let Ok(price) = price_str.as_str().parse::<f64>() {
                    // Gold price sanity check (current prices around 5000)
                    if price > 1500.0 && price < 10000.0 {
                        return Some(price);
                    }
                }
            }
        }
        None
    }

    fn extract_stop_loss(&self, message: &str) -> Option<f64> {
        if let Some(caps) = self.sl_pattern.captures(message) {
            if let Some(sl_str) = caps.get(1) {
                return sl_str.as_str().parse().ok();
            }
        }
        None
    }

    fn extract_take_profits(&self, message: &str) -> Vec<f64> {
        let mut tps = Vec::new();
        for caps in self.tp_pattern.captures_iter(message) {
            if let Some(tp_str) = caps.get(1) {
                if let Ok(tp) = tp_str.as_str().parse::<f64>() {
                    tps.push(tp);
                }
            }
        }
        tps
    }

    fn calculate_confidence(
        &self,
        direction: &TradeDirection,
        entry: &Option<f64>,
        sl: &Option<f64>,
        tps: &[f64],
    ) -> f64 {
        let mut confidence: f64 = 0.3; // Base confidence for having direction
        
        if entry.is_some() {
            confidence += 0.2;
        }
        if sl.is_some() {
            confidence += 0.25;
        }
        if !tps.is_empty() {
            confidence += 0.25;
        }
        
        // Validate SL/TP logic
        if let (Some(entry), Some(sl)) = (entry, sl) {
            let sl_valid = match direction {
                TradeDirection::Buy => *sl < *entry,   // SL should be below entry for buys
                TradeDirection::Sell => *sl > *entry,  // SL should be above entry for sells
            };
            if !sl_valid {
                confidence -= 0.2;
            }
        }
        
        confidence.clamp(0.0, 1.0)
    }
}

impl Default for SignalParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_signal() {
        let parser = SignalParser::new();
        
        let signal = parser.parse("BUY XAUUSD @2650 SL 2640 TP 2670").unwrap();
        assert_eq!(signal.direction, TradeDirection::Buy);
        assert_eq!(signal.entry_price, Some(2650.0));
        assert_eq!(signal.stop_loss, Some(2640.0));
        assert_eq!(signal.take_profit, vec![2670.0]);
    }

    #[test]
    fn test_parse_multiple_tp() {
        let parser = SignalParser::new();
        
        let signal = parser.parse("SELL GOLD @2700 SL: 2720 TP1: 2680 TP2: 2660").unwrap();
        assert_eq!(signal.direction, TradeDirection::Sell);
        assert_eq!(signal.take_profit.len(), 2);
    }

    #[test]
    fn test_parse_ben_gold_format() {
        let parser = SignalParser::new();
        
        // Real format from Ben Gold Trader channel
        let msg = "Sell Gold @5027.3-5035\n\nSl :5039\n\nTp1 : 5024\nTp2 :5020";
        let signal = parser.parse(msg).unwrap();
        assert_eq!(signal.direction, TradeDirection::Sell);
        assert_eq!(signal.entry_price, Some(5027.3));
        assert_eq!(signal.stop_loss, Some(5039.0));
        assert_eq!(signal.take_profit.len(), 2);
    }

    #[test]
    fn test_parse_informal_signal() {
        let parser = SignalParser::new();
        
        let signal = parser.parse("Going long on gold here @2650, stop at 2635").unwrap();
        assert_eq!(signal.direction, TradeDirection::Buy);
    }
}
