//! Sniper Bot API endpoints

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::sniper::{SolanaClient, WalletSession, PumpFunClient, PumpToken};
use crate::sniper::wallet::WalletSessionStore;

// ============ Request/Response Types ============

#[derive(Debug, Deserialize)]
pub struct ConnectWalletRequest {
    pub pubkey: String,
    pub network: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectWalletResponse {
    pub success: bool,
    pub session: Option<WalletSession>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenLookupRequest {
    pub mint_address: String,
}

#[derive(Debug, Serialize)]
pub struct TokenLookupResponse {
    pub success: bool,
    pub token: Option<TokenData>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenData {
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u8,
    pub supply: String,
    pub is_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct WalletBalanceResponse {
    pub success: bool,
    pub balance_sol: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<WalletSession>,
}

// ============ Handlers ============

/// POST /api/sniper/wallet/connect
pub async fn connect_wallet(
    wallet_store: web::Data<Arc<WalletSessionStore>>,
    body: web::Json<ConnectWalletRequest>,
) -> impl Responder {
    // Validate pubkey format
    if !SolanaClient::is_valid_address(&body.pubkey) {
        return HttpResponse::BadRequest().json(ConnectWalletResponse {
            success: false,
            session: None,
            error: Some("Invalid Solana address".to_string()),
        });
    }

    let network = body.network.clone().unwrap_or_else(|| "mainnet".to_string());
    
    // Create session
    let session = wallet_store.create_session(body.pubkey.clone(), network).await;

    HttpResponse::Ok().json(ConnectWalletResponse {
        success: true,
        session: Some(session),
        error: None,
    })
}

/// POST /api/sniper/wallet/disconnect
pub async fn disconnect_wallet(
    wallet_store: web::Data<Arc<WalletSessionStore>>,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if let Some(session_id) = body.get("session_id").and_then(|v| v.as_str()) {
        wallet_store.remove_session(session_id).await;
        HttpResponse::Ok().json(serde_json::json!({ "success": true }))
    } else {
        HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": "session_id required"
        }))
    }
}

/// GET /api/sniper/wallet/sessions
pub async fn list_sessions(
    wallet_store: web::Data<Arc<WalletSessionStore>>,
) -> impl Responder {
    let sessions = wallet_store.list_sessions().await;
    HttpResponse::Ok().json(SessionListResponse { sessions })
}

/// GET /api/sniper/wallet/balance/{pubkey}
pub async fn get_wallet_balance(
    path: web::Path<String>,
) -> impl Responder {
    let pubkey = path.into_inner();
    
    if !SolanaClient::is_valid_address(&pubkey) {
        return HttpResponse::BadRequest().json(WalletBalanceResponse {
            success: false,
            balance_sol: None,
            error: Some("Invalid address".to_string()),
        });
    }

    // Use mainnet by default
    let client = SolanaClient::mainnet();
    
    match client.get_balance(&pubkey) {
        Ok(balance) => HttpResponse::Ok().json(WalletBalanceResponse {
            success: true,
            balance_sol: Some(balance),
            error: None,
        }),
        Err(e) => HttpResponse::InternalServerError().json(WalletBalanceResponse {
            success: false,
            balance_sol: None,
            error: Some(e.to_string()),
        }),
    }
}

/// POST /api/sniper/token/lookup
pub async fn lookup_token(
    body: web::Json<TokenLookupRequest>,
) -> impl Responder {
    if !SolanaClient::is_valid_address(&body.mint_address) {
        return HttpResponse::BadRequest().json(TokenLookupResponse {
            success: false,
            token: None,
            error: Some("Invalid mint address".to_string()),
        });
    }

    let client = SolanaClient::mainnet();
    
    match client.get_token_info(&body.mint_address) {
        Ok(info) => HttpResponse::Ok().json(TokenLookupResponse {
            success: true,
            token: Some(TokenData {
                mint: info.mint,
                name: info.name,
                symbol: info.symbol,
                decimals: info.decimals,
                supply: info.supply.to_string(),
                is_verified: info.is_verified,
            }),
            error: None,
        }),
        Err(e) => HttpResponse::InternalServerError().json(TokenLookupResponse {
            success: false,
            token: None,
            error: Some(e.to_string()),
        }),
    }
}

/// GET /api/sniper/status
pub async fn sniper_status() -> impl Responder {
    // Check PumpFun accessibility
    let pumpfun_client = PumpFunClient::new();
    let pumpfun_accessible = pumpfun_client.health_check().await.unwrap_or(false);
    
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ready",
        "version": "0.1.0",
        "features": {
            "wallet_connect": true,
            "token_lookup": true,
            "pumpfun": pumpfun_accessible,
            "jito_bundles": false,
            "auto_sell": false
        },
        "pumpfun_status": if pumpfun_accessible { "connected" } else { "blocked (VPN required)" }
    }))
}

// ============ PumpFun Handlers ============

/// GET /api/sniper/pumpfun/coin/{mint}
pub async fn get_pumpfun_coin(
    path: web::Path<String>,
) -> impl Responder {
    let mint = path.into_inner();
    let client = PumpFunClient::new();
    
    match client.get_coin(&mint).await {
        Ok(token) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "token": token
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        }))
    }
}

/// GET /api/sniper/pumpfun/recent?limit=20
pub async fn get_recent_coins(
    query: web::Query<serde_json::Value>,
) -> impl Responder {
    let limit = query.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as u32;
    
    let client = PumpFunClient::new();
    
    match client.get_recent_coins(limit).await {
        Ok(tokens) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "tokens": tokens,
            "count": tokens.len()
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        }))
    }
}

/// GET /api/sniper/pumpfun/graduating?limit=20
pub async fn get_graduating_coins(
    query: web::Query<serde_json::Value>,
) -> impl Responder {
    let limit = query.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as u32;
    
    let client = PumpFunClient::new();
    
    match client.get_graduating_coins(limit).await {
        Ok(tokens) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "tokens": tokens,
            "count": tokens.len()
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        }))
    }
}

/// GET /api/sniper/pumpfun/trades/{mint}?limit=50
pub async fn get_coin_trades(
    path: web::Path<String>,
    query: web::Query<serde_json::Value>,
) -> impl Responder {
    let mint = path.into_inner();
    let limit = query.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as u32;
    
    let client = PumpFunClient::new();
    
    match client.get_trades(&mint, limit).await {
        Ok(trades) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "trades": trades,
            "count": trades.len()
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        }))
    }
}

/// POST /api/sniper/pumpfun/quote
#[derive(Debug, Deserialize)]
pub struct QuoteRequest {
    mint: String,
    sol_amount: Option<f64>,      // For buy quote (in SOL)
    token_amount: Option<u64>,    // For sell quote
    is_buy: bool,
}

pub async fn get_price_quote(
    body: web::Json<QuoteRequest>,
) -> impl Responder {
    let client = PumpFunClient::new();
    
    // First fetch the token to get bonding curve data
    let token = match client.get_coin(&body.mint).await {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({
            "success": false,
            "error": format!("Failed to fetch token: {}", e)
        }))
    };
    
    if body.is_buy {
        let sol_lamports = ((body.sol_amount.unwrap_or(0.1)) * 1_000_000_000.0) as u64;
        match client.calculate_buy_quote(&token, sol_lamports) {
            Ok(quote) => HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "quote": {
                    "input_sol": sol_lamports as f64 / 1_000_000_000.0,
                    "output_tokens": quote.output_amount,
                    "price_per_token_lamports": quote.price_per_token,
                    "price_impact_pct": quote.price_impact_pct
                },
                "token": {
                    "name": token.name,
                    "symbol": token.symbol,
                    "market_cap": token.usd_market_cap
                }
            })),
            Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    } else {
        let token_amount = body.token_amount.unwrap_or(1_000_000);
        match client.calculate_sell_quote(&token, token_amount) {
            Ok(quote) => HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "quote": {
                    "input_tokens": token_amount,
                    "output_sol": quote.output_amount as f64 / 1_000_000_000.0,
                    "price_per_token_lamports": quote.price_per_token,
                    "price_impact_pct": quote.price_impact_pct
                },
                "token": {
                    "name": token.name,
                    "symbol": token.symbol,
                    "market_cap": token.usd_market_cap
                }
            })),
            Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

// ============ Route Configuration ============

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/sniper")
            .route("/status", web::get().to(sniper_status))
            // Wallet routes
            .route("/wallet/connect", web::post().to(connect_wallet))
            .route("/wallet/disconnect", web::post().to(disconnect_wallet))
            .route("/wallet/sessions", web::get().to(list_sessions))
            .route("/wallet/balance/{pubkey}", web::get().to(get_wallet_balance))
            // Token routes
            .route("/token/lookup", web::post().to(lookup_token))
            // PumpFun routes
            .route("/pumpfun/coin/{mint}", web::get().to(get_pumpfun_coin))
            .route("/pumpfun/recent", web::get().to(get_recent_coins))
            .route("/pumpfun/graduating", web::get().to(get_graduating_coins))
            .route("/pumpfun/trades/{mint}", web::get().to(get_coin_trades))
            .route("/pumpfun/quote", web::post().to(get_price_quote))
    );
}
