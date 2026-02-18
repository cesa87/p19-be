use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

use super::signal_parser::{SignalParser, TradeSignal};

/// Fetches messages from public Telegram channels using web preview
/// Note: For full API access, you'd need MTProto client with user auth
pub struct ChannelMonitor {
    client: Client,
    parser: SignalParser,
}

/// A parsed signal with timestamp for historical analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalSignal {
    pub signal: TradeSignal,
    pub timestamp: i64,
    pub message_id: i64,
    pub channel_username: String,
}

/// Analysis results from historical signals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalAnalysis {
    pub total_signals: usize,
    pub buy_signals: usize,
    pub sell_signals: usize,
    pub avg_sl_pips: Option<f64>,
    pub avg_tp_pips: Option<f64>,
    pub avg_risk_reward: Option<f64>,
    pub signals_by_hour: Vec<(u32, usize)>,  // Hour of day -> count
    pub signals_by_day: Vec<(String, usize)>, // Day of week -> count
    pub common_entry_levels: Vec<f64>,
}

impl ChannelMonitor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            parser: SignalParser::new(),
        }
    }

    /// Analyze a list of historical signals
    pub fn analyze_signals(&self, signals: &[HistoricalSignal]) -> SignalAnalysis {
        let mut buy_count = 0;
        let mut sell_count = 0;
        let mut sl_pips_total = 0.0;
        let mut sl_count = 0;
        let mut tp_pips_total = 0.0;
        let mut tp_count = 0;
        let mut risk_reward_total = 0.0;
        let mut rr_count = 0;
        
        for hs in signals {
            match hs.signal.direction {
                super::signal_parser::TradeDirection::Buy => buy_count += 1,
                super::signal_parser::TradeDirection::Sell => sell_count += 1,
            }
            
            // Calculate SL pips if we have entry and SL
            if let (Some(entry), Some(sl)) = (hs.signal.entry_price, hs.signal.stop_loss) {
                let sl_pips = (entry - sl).abs();
                sl_pips_total += sl_pips;
                sl_count += 1;
                
                // Calculate TP pips and risk/reward
                if let Some(tp) = hs.signal.take_profit.first() {
                    let tp_pips = (tp - entry).abs();
                    tp_pips_total += tp_pips;
                    tp_count += 1;
                    
                    if sl_pips > 0.0 {
                        risk_reward_total += tp_pips / sl_pips;
                        rr_count += 1;
                    }
                }
            }
        }

        SignalAnalysis {
            total_signals: signals.len(),
            buy_signals: buy_count,
            sell_signals: sell_count,
            avg_sl_pips: if sl_count > 0 { Some(sl_pips_total / sl_count as f64) } else { None },
            avg_tp_pips: if tp_count > 0 { Some(tp_pips_total / tp_count as f64) } else { None },
            avg_risk_reward: if rr_count > 0 { Some(risk_reward_total / rr_count as f64) } else { None },
            signals_by_hour: vec![], // Would need timestamp parsing
            signals_by_day: vec![],
            common_entry_levels: vec![],
        }
    }

    /// Parse multiple messages and extract signals
    pub fn parse_messages(&self, messages: &[(i64, i64, String)], channel: &str) -> Vec<HistoricalSignal> {
        let mut signals = Vec::new();
        
        for (msg_id, timestamp, text) in messages {
            if let Some(signal) = self.parser.parse(text) {
                signals.push(HistoricalSignal {
                    signal,
                    timestamp: *timestamp,
                    message_id: *msg_id,
                    channel_username: channel.to_string(),
                });
            }
        }
        
        info!("Parsed {} signals from {} messages", signals.len(), messages.len());
        signals
    }
}

impl Default for ChannelMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_signals() {
        let monitor = ChannelMonitor::new();
        let parser = SignalParser::new();
        
        let messages = vec![
            (1, 1700000000, "Buy Gold @2650 SL 2640 TP 2670".to_string()),
            (2, 1700001000, "Sell Gold @2680 SL 2695 TP 2660".to_string()),
            (3, 1700002000, "Buy Gold @2655 SL 2645 TP 2680".to_string()),
        ];
        
        let signals = monitor.parse_messages(&messages, "test_channel");
        assert_eq!(signals.len(), 3);
        
        let analysis = monitor.analyze_signals(&signals);
        assert_eq!(analysis.total_signals, 3);
        assert_eq!(analysis.buy_signals, 2);
        assert_eq!(analysis.sell_signals, 1);
    }
}
