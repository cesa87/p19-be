//! Solana RPC client wrapper using reqwest
//! Avoids heavy solana-sdk dependency conflicts by calling RPC directly

use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};

/// Token metadata from on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u8,
    pub supply: u64,
    pub holder_count: Option<u32>,
    pub is_verified: bool,
}

/// Solana RPC JSON-RPC request
#[derive(Serialize)]
struct RpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: T,
}

/// Generic RPC response wrapper
#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    message: String,
}

/// Balance response
#[derive(Deserialize)]
struct BalanceResult {
    value: u64,
}

/// Token supply response
#[derive(Deserialize)]
struct TokenSupplyResult {
    value: TokenSupplyValue,
}

#[derive(Deserialize)]
struct TokenSupplyValue {
    amount: String,
    decimals: u8,
}

/// Solana RPC client using reqwest
pub struct SolanaClient {
    rpc_url: String,
    client: reqwest::Client,
}

impl SolanaClient {
    /// Create new client with RPC endpoint
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Create client for mainnet
    pub fn mainnet() -> Self {
        Self::new("https://api.mainnet-beta.solana.com")
    }

    /// Create client for devnet  
    pub fn devnet() -> Self {
        Self::new("https://api.devnet.solana.com")
    }

    /// Get SOL balance for a wallet (blocking for simplicity)
    pub fn get_balance(&self, pubkey: &str) -> Result<f64> {
        if !Self::is_valid_address(pubkey) {
            return Err(anyhow!("Invalid pubkey"));
        }

        let request = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getBalance",
            params: vec![pubkey],
        };

        let response: RpcResponse<BalanceResult> = reqwest::blocking::Client::new()
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .map_err(|e| anyhow!("RPC request failed: {}", e))?
            .json()
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;

        if let Some(err) = response.error {
            return Err(anyhow!("RPC error: {}", err.message));
        }

        let lamports = response.result
            .ok_or_else(|| anyhow!("No result in response"))?
            .value;

        Ok(lamports as f64 / 1_000_000_000.0)
    }

    /// Lookup token by mint address
    pub fn get_token_info(&self, mint_address: &str) -> Result<TokenInfo> {
        if !Self::is_valid_address(mint_address) {
            return Err(anyhow!("Invalid mint address"));
        }

        let request = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getTokenSupply",
            params: vec![mint_address],
        };

        let response: RpcResponse<TokenSupplyResult> = reqwest::blocking::Client::new()
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .map_err(|e| anyhow!("RPC request failed: {}", e))?
            .json()
            .map_err(|e| anyhow!("Failed to parse response: {}", e))?;

        let (decimals, supply) = if let Some(result) = response.result {
            let supply = result.value.amount.parse::<u64>().unwrap_or(0);
            (result.value.decimals, supply)
        } else {
            (9, 0)
        };

        Ok(TokenInfo {
            mint: mint_address.to_string(),
            name: None,
            symbol: None,
            decimals,
            supply,
            holder_count: None,
            is_verified: false,
        })
    }

    /// Check if address is valid Solana pubkey (base58, 32-44 chars)
    pub fn is_valid_address(address: &str) -> bool {
        if address.len() < 32 || address.len() > 44 {
            return false;
        }
        bs58::decode(address).into_vec().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_address() {
        assert!(SolanaClient::is_valid_address("So11111111111111111111111111111111111111112"));
        assert!(!SolanaClient::is_valid_address("invalid"));
        assert!(!SolanaClient::is_valid_address("abc")); // too short
    }
}
