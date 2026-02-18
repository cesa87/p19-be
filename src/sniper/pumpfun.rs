//! PumpFun API integration
//! 
//! Provides access to PumpFun token data and bonding curve information.
//! Note: PumpFun may be geo-restricted - requires VPN in some regions.

use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use reqwest::Client;

const PUMPFUN_API_BASE: &str = "https://frontend-api.pump.fun";

/// PumpFun token/coin data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpToken {
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub description: Option<String>,
    pub image_uri: Option<String>,
    pub metadata_uri: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub bonding_curve: Option<String>,
    pub associated_bonding_curve: Option<String>,
    pub creator: String,
    pub created_timestamp: i64,
    pub complete: bool,
    pub virtual_sol_reserves: Option<u64>,
    pub virtual_token_reserves: Option<u64>,
    pub total_supply: Option<u64>,
    pub market_cap: Option<f64>,
    pub usd_market_cap: Option<f64>,
    pub reply_count: Option<u32>,
    pub last_reply: Option<i64>,
    pub king_of_the_hill_timestamp: Option<i64>,
    pub raydium_pool: Option<String>,
}

/// Trade/transaction on PumpFun
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpTrade {
    pub signature: String,
    pub mint: String,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub is_buy: bool,
    pub user: String,
    pub timestamp: i64,
    pub slot: u64,
}

/// Bonding curve price quote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceQuote {
    pub input_amount: u64,
    pub output_amount: u64,
    pub price_per_token: f64,
    pub price_impact_pct: f64,
}

/// PumpFun API client
pub struct PumpFunClient {
    client: Client,
    base_url: String,
}

impl PumpFunClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: PUMPFUN_API_BASE.to_string(),
        }
    }

    /// Get token/coin info by mint address
    pub async fn get_coin(&self, mint: &str) -> Result<PumpToken> {
        let url = format!("{}/coins/{}", self.base_url, mint);
        
        let response = self.client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("API error {}: {}", status, body));
        }

        response.json::<PumpToken>()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    /// Get recently created tokens
    pub async fn get_recent_coins(&self, limit: u32) -> Result<Vec<PumpToken>> {
        let url = format!("{}/coins?limit={}&sort=created_timestamp&order=DESC", 
            self.base_url, limit.min(50));
        
        let response = self.client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow!("API error: {}", status));
        }

        response.json::<Vec<PumpToken>>()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    /// Get tokens about to graduate (high market cap, not yet on Raydium)
    pub async fn get_graduating_coins(&self, limit: u32) -> Result<Vec<PumpToken>> {
        let url = format!("{}/coins?limit={}&sort=market_cap&order=DESC&complete=false", 
            self.base_url, limit.min(50));
        
        let response = self.client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("API error: {}", response.status()));
        }

        response.json::<Vec<PumpToken>>()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    /// Get recent trades for a token
    pub async fn get_trades(&self, mint: &str, limit: u32) -> Result<Vec<PumpTrade>> {
        let url = format!("{}/trades/{}?limit={}", self.base_url, mint, limit.min(200));
        
        let response = self.client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| anyhow!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("API error: {}", response.status()));
        }

        response.json::<Vec<PumpTrade>>()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    /// Calculate buy quote from bonding curve
    /// Returns estimated tokens received for given SOL amount
    pub fn calculate_buy_quote(
        &self, 
        token: &PumpToken, 
        sol_amount_lamports: u64
    ) -> Result<PriceQuote> {
        let virtual_sol = token.virtual_sol_reserves
            .ok_or_else(|| anyhow!("Token missing virtual_sol_reserves"))?;
        let virtual_token = token.virtual_token_reserves
            .ok_or_else(|| anyhow!("Token missing virtual_token_reserves"))?;

        if virtual_sol == 0 || virtual_token == 0 {
            return Err(anyhow!("Invalid reserves"));
        }

        // Constant product formula: k = x * y
        // After buy: (x + dx) * (y - dy) = k
        // dy = y - k / (x + dx)
        let k = (virtual_sol as u128) * (virtual_token as u128);
        let new_sol = virtual_sol + sol_amount_lamports;
        let new_token = (k / new_sol as u128) as u64;
        let tokens_out = virtual_token.saturating_sub(new_token);

        // Apply 1% fee
        let tokens_after_fee = (tokens_out as f64 * 0.99) as u64;

        let price_before = virtual_sol as f64 / virtual_token as f64;
        let price_after = new_sol as f64 / new_token as f64;
        let price_impact = ((price_after - price_before) / price_before) * 100.0;

        Ok(PriceQuote {
            input_amount: sol_amount_lamports,
            output_amount: tokens_after_fee,
            price_per_token: sol_amount_lamports as f64 / tokens_after_fee as f64,
            price_impact_pct: price_impact,
        })
    }

    /// Calculate sell quote from bonding curve
    /// Returns estimated SOL received for given token amount
    pub fn calculate_sell_quote(
        &self,
        token: &PumpToken,
        token_amount: u64
    ) -> Result<PriceQuote> {
        let virtual_sol = token.virtual_sol_reserves
            .ok_or_else(|| anyhow!("Token missing virtual_sol_reserves"))?;
        let virtual_token = token.virtual_token_reserves
            .ok_or_else(|| anyhow!("Token missing virtual_token_reserves"))?;

        if virtual_sol == 0 || virtual_token == 0 {
            return Err(anyhow!("Invalid reserves"));
        }

        // Constant product formula for sell
        let k = (virtual_sol as u128) * (virtual_token as u128);
        let new_token = virtual_token + token_amount;
        let new_sol = (k / new_token as u128) as u64;
        let sol_out = virtual_sol.saturating_sub(new_sol);

        // Apply 1% fee
        let sol_after_fee = (sol_out as f64 * 0.99) as u64;

        let price_before = virtual_sol as f64 / virtual_token as f64;
        let price_after = new_sol as f64 / new_token as f64;
        let price_impact = ((price_before - price_after) / price_before) * 100.0;

        Ok(PriceQuote {
            input_amount: token_amount,
            output_amount: sol_after_fee,
            price_per_token: sol_after_fee as f64 / token_amount as f64,
            price_impact_pct: price_impact,
        })
    }

    /// Check if API is accessible (useful for VPN check)
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/coins?limit=1", self.base_url);
        
        match self.client.get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await 
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

impl Default for PumpFunClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buy_quote_calculation() {
        let client = PumpFunClient::new();
        let token = PumpToken {
            mint: "test".to_string(),
            name: "Test".to_string(),
            symbol: "TEST".to_string(),
            description: None,
            image_uri: None,
            metadata_uri: None,
            twitter: None,
            telegram: None,
            website: None,
            bonding_curve: None,
            associated_bonding_curve: None,
            creator: "creator".to_string(),
            created_timestamp: 0,
            complete: false,
            virtual_sol_reserves: Some(30_000_000_000), // 30 SOL
            virtual_token_reserves: Some(1_000_000_000_000), // 1B tokens
            total_supply: Some(1_000_000_000_000),
            market_cap: None,
            usd_market_cap: None,
            reply_count: None,
            last_reply: None,
            king_of_the_hill_timestamp: None,
            raydium_pool: None,
        };

        let quote = client.calculate_buy_quote(&token, 1_000_000_000).unwrap(); // 1 SOL
        assert!(quote.output_amount > 0);
        assert!(quote.price_impact_pct > 0.0);
    }
}
