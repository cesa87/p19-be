use tracing::{info, warn};
use serde::{Deserialize, Serialize};
use crate::telegram::signal_parser::{TradeSignal, TradeDirection};

/// Discovered strategy pattern from AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredStrategy {
    pub name: String,
    pub description: String,
    pub entry_rules: Vec<StrategyRule>,
    pub exit_rules: Vec<StrategyRule>,
    pub risk_management: RiskManagement,
    pub observations: Vec<String>,
    pub confidence_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRule {
    pub indicator: String,
    pub condition: String,
    pub value: Option<f64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskManagement {
    pub suggested_sl_pips: f64,
    pub suggested_tp_pips: f64,
    pub risk_reward_ratio: f64,
    pub position_sizing: String,
}

/// Signal enriched with market context
#[derive(Debug, Clone, Serialize)]
pub struct EnrichedSignal {
    pub signal: TradeSignal,
    pub market_context: Option<MarketContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketContext {
    pub price_at_signal: f64,
    pub rsi_14: Option<f64>,
    pub ema_20: Option<f64>,
    pub ema_50: Option<f64>,
    pub atr_14: Option<f64>,
    pub hour_of_day: Option<u8>,
    pub day_of_week: Option<String>,
}

/// AI Strategy Discovery Engine
pub struct StrategyDiscovery {
    api_key: String,
    client: reqwest::Client,
}

impl StrategyDiscovery {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Analyze signals and discover trading patterns using GPT-4
    pub async fn discover_strategy(&self, signals: &[TradeSignal]) -> Result<DiscoveredStrategy, String> {
        info!("Analyzing {} signals for pattern discovery", signals.len());
        
        if signals.is_empty() {
            return Err("No signals to analyze".to_string());
        }

        // Prepare signal summary for the AI
        let signal_summary = self.prepare_signal_summary(signals);
        
        // Build the prompt
        let prompt = self.build_analysis_prompt(&signal_summary, signals);
        
        // Call GPT-4
        let response = self.call_openai(&prompt).await?;
        
        // Parse the response into a strategy
        self.parse_strategy_response(&response)
    }

    fn prepare_signal_summary(&self, signals: &[TradeSignal]) -> SignalSummary {
        let buy_signals: Vec<_> = signals.iter().filter(|s| s.direction == TradeDirection::Buy).collect();
        let sell_signals: Vec<_> = signals.iter().filter(|s| s.direction == TradeDirection::Sell).collect();
        
        let avg_buy_price = if !buy_signals.is_empty() {
            buy_signals.iter().filter_map(|s| s.entry_price).sum::<f64>() / buy_signals.len() as f64
        } else { 0.0 };
        
        let avg_sell_price = if !sell_signals.is_empty() {
            sell_signals.iter().filter_map(|s| s.entry_price).sum::<f64>() / sell_signals.len() as f64
        } else { 0.0 };

        // Calculate SL/TP stats
        let sl_distances: Vec<f64> = signals.iter()
            .filter_map(|s| {
                match (s.entry_price, s.stop_loss) {
                    (Some(e), Some(sl)) => Some((e - sl).abs()),
                    _ => None,
                }
            })
            .collect();
        
        let tp_distances: Vec<f64> = signals.iter()
            .filter_map(|s| {
                match (s.entry_price, &s.take_profit) {
                    (Some(e), tps) if !tps.is_empty() => Some((tps[0] - e).abs()),
                    _ => None,
                }
            })
            .collect();

        SignalSummary {
            total_signals: signals.len(),
            buy_count: buy_signals.len(),
            sell_count: sell_signals.len(),
            avg_buy_price,
            avg_sell_price,
            avg_sl_pips: if !sl_distances.is_empty() { 
                sl_distances.iter().sum::<f64>() / sl_distances.len() as f64 * 10.0 
            } else { 0.0 },
            avg_tp_pips: if !tp_distances.is_empty() { 
                tp_distances.iter().sum::<f64>() / tp_distances.len() as f64 * 10.0 
            } else { 0.0 },
        }
    }

    fn build_analysis_prompt(&self, summary: &SignalSummary, signals: &[TradeSignal]) -> String {
        // Build a list of recent signals for context
        let signal_examples: String = signals.iter()
            .take(30) // Limit to avoid token limits
            .map(|s| {
                format!(
                    "- {} @ {:.1} (SL: {}, TP: {})",
                    s.direction,
                    s.entry_price.unwrap_or(0.0),
                    s.stop_loss.map(|v| format!("{:.1}", v)).unwrap_or("N/A".to_string()),
                    if s.take_profit.is_empty() { "N/A".to_string() } 
                    else { s.take_profit.iter().map(|v| format!("{:.1}", v)).collect::<Vec<_>>().join(", ") }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(r#"You are an expert quantitative trading analyst specializing in gold (XAUUSD) trading. Analyze the following trading signals from a successful Telegram channel and discover the underlying trading strategy.

## Signal Statistics
- Total Signals: {}
- Buy Signals: {} ({:.1}%)
- Sell Signals: {} ({:.1}%)
- Average Buy Entry Price: ${:.2}
- Average Sell Entry Price: ${:.2}
- Average Stop Loss: {:.1} pips
- Average Take Profit: {:.1} pips
- Average Risk:Reward Ratio: {:.2}:1

## Sample Signals
{}

## Your Task
Analyze these signals and identify:
1. What technical conditions likely trigger BUY signals
2. What technical conditions likely trigger SELL signals
3. Risk management patterns (SL/TP placement logic)
4. Any observable price level preferences (round numbers, support/resistance)
5. Overall strategy characteristics

Respond in this exact JSON format:
{{
  "name": "Strategy name based on observed patterns",
  "description": "2-3 sentence description of the strategy",
  "entry_rules": [
    {{"indicator": "RSI", "condition": "less_than", "value": 30, "description": "RSI below 30 indicates oversold"}},
    {{"indicator": "Price", "condition": "near_round_number", "value": 50, "description": "Price within 50 pips of round number"}}
  ],
  "exit_rules": [
    {{"indicator": "TP", "condition": "fixed_pips", "value": 10, "description": "Take profit at 10 pips"}}
  ],
  "risk_management": {{
    "suggested_sl_pips": 15,
    "suggested_tp_pips": 10,
    "risk_reward_ratio": 0.67,
    "position_sizing": "1-2% risk per trade"
  }},
  "observations": [
    "Key observation 1",
    "Key observation 2"
  ],
  "confidence_score": 0.75
}}

Only respond with valid JSON, no additional text."#,
            summary.total_signals,
            summary.buy_count,
            (summary.buy_count as f64 / summary.total_signals as f64) * 100.0,
            summary.sell_count,
            (summary.sell_count as f64 / summary.total_signals as f64) * 100.0,
            summary.avg_buy_price,
            summary.avg_sell_price,
            summary.avg_sl_pips,
            summary.avg_tp_pips,
            if summary.avg_sl_pips > 0.0 { summary.avg_tp_pips / summary.avg_sl_pips } else { 0.0 },
            signal_examples
        )
    }

    async fn call_openai(&self, prompt: &str) -> Result<String, String> {
        let request_body = serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "You are an expert quantitative trading analyst. Always respond with valid JSON only."
                },
                {
                    "role": "user", 
                    "content": prompt
                }
            ],
            "temperature": 0.3,
            "max_tokens": 2000
        });

        info!("Calling OpenAI API for strategy analysis...");

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to call OpenAI: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            warn!("OpenAI API error: {}", error_text);
            return Err(format!("OpenAI API error: {}", error_text));
        }

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

        let content = response_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or("No content in OpenAI response")?
            .to_string();

        info!("Received strategy analysis from OpenAI");
        Ok(content)
    }

    fn parse_strategy_response(&self, response: &str) -> Result<DiscoveredStrategy, String> {
        // Clean up the response - remove markdown code blocks if present
        let cleaned = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        serde_json::from_str(cleaned)
            .map_err(|e| format!("Failed to parse strategy response: {}. Response was: {}", e, cleaned))
    }
}

#[derive(Debug)]
struct SignalSummary {
    total_signals: usize,
    buy_count: usize,
    sell_count: usize,
    avg_buy_price: f64,
    avg_sell_price: f64,
    avg_sl_pips: f64,
    avg_tp_pips: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_summary() {
        let signals = vec![
            TradeSignal {
                direction: TradeDirection::Buy,
                symbol: "XAU_USD".to_string(),
                entry_price: Some(2000.0),
                stop_loss: Some(1990.0),
                take_profit: vec![2010.0],
                lot_size: None,
                raw_message: "".to_string(),
                confidence: 1.0,
            },
            TradeSignal {
                direction: TradeDirection::Sell,
                symbol: "XAU_USD".to_string(),
                entry_price: Some(2020.0),
                stop_loss: Some(2030.0),
                take_profit: vec![2010.0],
                lot_size: None,
                raw_message: "".to_string(),
                confidence: 1.0,
            },
        ];

        let discovery = StrategyDiscovery::new("test".to_string());
        let summary = discovery.prepare_signal_summary(&signals);
        
        assert_eq!(summary.total_signals, 2);
        assert_eq!(summary.buy_count, 1);
        assert_eq!(summary.sell_count, 1);
    }
}
