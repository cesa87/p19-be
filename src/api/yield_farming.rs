//! Yield Farming API endpoints

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize, Serializer};
use sqlx::types::BigDecimal;
use chrono::{DateTime, Utc};
use std::str::FromStr;

use crate::db::DbPool;

// Custom serializer for BigDecimal -> String
fn serialize_bigdecimal<S>(value: &BigDecimal, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

fn serialize_bigdecimal_opt<S>(value: &Option<BigDecimal>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(bd) => serializer.serialize_str(&bd.to_string()),
        None => serializer.serialize_none(),
    }
}

// ============ Response Types ============

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Protocol {
    pub id: String,
    pub name: String,
    pub protocol_type: String,
    pub enabled: bool,
    pub risk_score: i32,
    #[serde(serialize_with = "serialize_bigdecimal_opt")]
    pub tvl_usd: Option<BigDecimal>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Pool {
    pub id: String,
    pub protocol_id: String,
    pub protocol_name: String,
    pub protocol_risk_score: i32,
    pub pool_name: String,
    pub pool_address: String,
    pub token_a: String,
    pub token_b: Option<String>,
    pub current_apy: f64,
    pub apy_7d_avg: Option<f64>,
    pub apy_30d_avg: Option<f64>,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub tvl_usd: BigDecimal,
    pub impermanent_loss_risk: String,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Position {
    pub id: String,
    pub pool_id: String,
    pub pool_name: String,
    pub wallet_address: String,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub deposit_amount_usd: BigDecimal,
    #[serde(serialize_with = "serialize_bigdecimal_opt")]
    pub current_value_usd: Option<BigDecimal>,
    #[serde(serialize_with = "serialize_bigdecimal_opt")]
    pub unrealized_pnl_usd: Option<BigDecimal>,
    #[serde(serialize_with = "serialize_bigdecimal_opt")]
    pub rewards_earned_usd: Option<BigDecimal>,
    pub entry_apy: f64,
    pub entry_date: DateTime<Utc>,
    pub last_harvest: Option<DateTime<Utc>>,
    pub harvest_count: i32,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Rotation {
    pub id: String,
    pub from_pool_id: Option<String>,
    pub from_pool_name: Option<String>,
    pub to_pool_id: String,
    pub to_pool_name: String,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub amount_usd: BigDecimal,
    pub from_apy: Option<f64>,
    pub to_apy: f64,
    pub apy_improvement: Option<f64>,
    pub rotation_reason: String,
    pub executed_at: DateTime<Utc>,
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct PortfolioSummary {
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub total_deposited_usd: BigDecimal,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub total_current_value_usd: BigDecimal,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub total_unrealized_pnl_usd: BigDecimal,
    #[serde(serialize_with = "serialize_bigdecimal")]
    pub total_rewards_earned_usd: BigDecimal,
    pub weighted_avg_apy: f64,
    pub active_positions: i64,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteRotationRequest {
    #[serde(rename = "toPoolId")]
    pub to_pool_id: String,
    #[serde(rename = "amountUsd")]
    pub amount_usd: f64,
    pub reason: String,
    #[serde(rename = "fromPositionId")]
    pub from_position_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub pool_id: String,
    pub wallet_address: String,
    pub amount_usd: f64,
}

#[derive(Debug, Deserialize)]
pub struct WithdrawRequest {
    pub position_id: String,
    pub amount_usd: Option<f64>, // None = full withdrawal
}

#[derive(Debug, Deserialize)]
pub struct HarvestRequest {
    pub position_id: String,
}

#[derive(Debug, Deserialize)]
pub struct OptimizeRequest {
    pub wallet_address: String,
    pub total_amount_usd: f64,
    pub risk_tolerance: Option<String>, // "low", "medium", "high"
}

#[derive(Debug, Serialize)]
pub struct OptimizeResponse {
    pub allocations: Vec<AllocationRecommendation>,
    pub expected_apy: f64,
    pub risk_score: f64,
}

#[derive(Debug, Serialize)]
pub struct AllocationRecommendation {
    pub pool_id: String,
    pub pool_name: String,
    pub protocol: String,
    pub amount_usd: f64,
    pub percentage: f64,
    pub apy: f64,
    pub reason: String,
}

// ============ Handlers ============

/// GET /api/yield/protocols
pub async fn get_protocols(pool: web::Data<DbPool>) -> impl Responder {
    let result = sqlx::query_as::<_, Protocol>(
        "SELECT id::text, name, protocol_type, enabled, risk_score, tvl_usd, last_updated 
         FROM yield_protocols 
         ORDER BY enabled DESC, name ASC"
    )
    .fetch_all(pool.get_ref())
    .await;

    match result {
        Ok(protocols) => HttpResponse::Ok().json(protocols),
        Err(e) => {
            eprintln!("Failed to fetch protocols: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch protocols"
            }))
        }
    }
}

/// GET /api/yield/pools/top?limit=20
pub async fn get_top_pools(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let limit: i64 = query.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let result = sqlx::query_as::<_, Pool>(
        "SELECT 
            p.id::text, p.protocol_id::text, pr.name as protocol_name, pr.risk_score as protocol_risk_score,
            p.pool_name, p.pool_address, p.token_a, p.token_b,
            p.current_apy::float8, p.apy_7d_avg::float8, p.apy_30d_avg::float8, p.tvl_usd,
            p.impermanent_loss_risk, p.last_updated
         FROM yield_pools p
         JOIN yield_protocols pr ON p.protocol_id = pr.id
         WHERE p.active = true AND pr.enabled = true
         ORDER BY p.current_apy DESC
         LIMIT $1"
    )
    .bind(limit)
    .fetch_all(pool.get_ref())
    .await;

    match result {
        Ok(pools) => HttpResponse::Ok().json(pools),
        Err(e) => {
            eprintln!("Failed to fetch pools: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch pools"
            }))
        }
    }
}

/// GET /api/yield/positions?wallet=<address>
pub async fn get_positions(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let wallet = match query.get("wallet") {
        Some(w) => w,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "wallet parameter required"
            }));
        }
    };

    let result = sqlx::query_as::<_, Position>(
        "SELECT 
            pos.id::text, pos.pool_id::text, p.pool_name, pos.wallet_address,
            pos.deposit_amount_usd, pos.current_value_usd, pos.unrealized_pnl_usd,
            pos.rewards_earned_usd, pos.entry_apy::float8, pos.entered_at as entry_date,
            pos.last_harvest_at as last_harvest, pos.harvest_count
         FROM yield_positions pos
         JOIN yield_pools p ON pos.pool_id = p.id
         WHERE pos.wallet_address = $1 AND pos.status = 'active'
         ORDER BY pos.entered_at DESC"
    )
    .bind(wallet)
    .fetch_all(pool.get_ref())
    .await;

    match result {
        Ok(positions) => HttpResponse::Ok().json(positions),
        Err(e) => {
            eprintln!("Failed to fetch positions: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch positions"
            }))
        }
    }
}

/// GET /api/yield/rotations?wallet=<address>&limit=10
pub async fn get_rotations(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let wallet = query.get("wallet");
    let limit: i64 = query.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let sql = if let Some(wallet_addr) = wallet {
        sqlx::query_as::<_, Rotation>(
            "SELECT 
                r.id::text, r.from_pool_id::text, p1.pool_name as from_pool_name,
                r.to_pool_id::text, p2.pool_name as to_pool_name,
                r.amount_usd, r.from_apy::float8, r.to_apy::float8, r.apy_improvement::float8,
                r.rotation_reason, r.executed_at, r.success
             FROM yield_rotations r
             LEFT JOIN yield_pools p1 ON r.from_pool_id = p1.id
             JOIN yield_pools p2 ON r.to_pool_id = p2.id
             WHERE r.wallet_address = $1
             ORDER BY r.executed_at DESC
             LIMIT $2"
        )
        .bind(wallet_addr)
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    } else {
        sqlx::query_as::<_, Rotation>(
            "SELECT 
                r.id::text, r.from_pool_id::text, p1.pool_name as from_pool_name,
                r.to_pool_id::text, p2.pool_name as to_pool_name,
                r.amount_usd, r.from_apy::float8, r.to_apy::float8, r.apy_improvement::float8,
                r.rotation_reason, r.executed_at, r.success
             FROM yield_rotations r
             LEFT JOIN yield_pools p1 ON r.from_pool_id = p1.id
             JOIN yield_pools p2 ON r.to_pool_id = p2.id
             ORDER BY r.executed_at DESC
             LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool.get_ref())
        .await
    };

    match sql {
        Ok(rotations) => HttpResponse::Ok().json(rotations),
        Err(e) => {
            eprintln!("Failed to fetch rotations: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch rotations"
            }))
        }
    }
}

/// GET /api/yield/portfolio?wallet=<address>
pub async fn get_portfolio_summary(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let wallet = match query.get("wallet") {
        Some(w) => w,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "wallet parameter required"
            }));
        }
    };

    let result = sqlx::query_as::<_, (BigDecimal, BigDecimal, BigDecimal, BigDecimal, f64, i64)>(
        "SELECT 
            COALESCE(SUM(deposit_amount_usd), 0) as total_deposited,
            COALESCE(SUM(current_value_usd), 0) as total_current,
            COALESCE(SUM(unrealized_pnl_usd), 0) as total_pnl,
            COALESCE(SUM(rewards_earned_usd), 0) as total_rewards,
            COALESCE(AVG(entry_apy)::float8, 0) as avg_apy,
            COUNT(*) as position_count
         FROM yield_positions
         WHERE wallet_address = $1 AND status = 'active'"
    )
    .bind(wallet)
    .fetch_one(pool.get_ref())
    .await;

    match result {
        Ok((deposited, current, pnl, rewards, apy, count)) => {
            HttpResponse::Ok().json(PortfolioSummary {
                total_deposited_usd: deposited,
                total_current_value_usd: current,
                total_unrealized_pnl_usd: pnl,
                total_rewards_earned_usd: rewards,
                weighted_avg_apy: apy,
                active_positions: count,
            })
        }
        Err(e) => {
            eprintln!("Failed to fetch portfolio: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch portfolio summary"
            }))
        }
    }
}

/// POST /api/yield/rotations/execute
pub async fn execute_rotation(
    pool: web::Data<DbPool>,
    body: web::Json<ExecuteRotationRequest>,
) -> impl Responder {
    // TODO: Implement actual rotation logic with Solana integration
    // For now, just log the rotation to database
    
    let rotation_id = uuid::Uuid::new_v4().to_string();
    
    // Get pool info
    let pool_info = sqlx::query_as::<_, (String, f64)>(
        "SELECT pool_name, current_apy::float8 FROM yield_pools WHERE id::text = $1"
    )
    .bind(&body.to_pool_id)
    .fetch_optional(pool.get_ref())
    .await;

    let (pool_name, to_apy) = match pool_info {
        Ok(Some(info)) => info,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "error": "Pool not found"
            }));
        }
        Err(e) => {
            eprintln!("Failed to fetch pool: {}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Database error"
            }));
        }
    };

    // Insert rotation record
    let result = sqlx::query(
        "INSERT INTO yield_rotations 
         (id, from_pool_id, to_pool_id, wallet_address, amount_usd, from_apy, to_apy, 
          rotation_reason, executed_at, success, tx_signature)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
    )
    .bind(&rotation_id)
    .bind(&body.from_position_id)
    .bind(&body.to_pool_id)
    .bind("demo_wallet") // TODO: Get from session
    .bind(BigDecimal::from(body.amount_usd as i64))
    .bind(None::<f64>)
    .bind(to_apy)
    .bind(&body.reason)
    .bind(Utc::now())
    .bind(false) // Will be true after actual tx
    .bind(None::<String>)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "rotation_id": rotation_id,
            "message": "Rotation queued (transaction signing not yet implemented)"
        })),
        Err(e) => {
            eprintln!("Failed to insert rotation: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to execute rotation"
            }))
        }
    }
}

/// POST /api/yield/positions/deposit
pub async fn deposit_position(
    pool: web::Data<DbPool>,
    body: web::Json<DepositRequest>,
) -> impl Responder {
    // TODO: Implement actual deposit with Solana
    HttpResponse::Ok().json(serde_json::json!({
        "success": false,
        "message": "Deposit functionality coming soon"
    }))
}

/// POST /api/yield/positions/withdraw
pub async fn withdraw_position(
    pool: web::Data<DbPool>,
    body: web::Json<WithdrawRequest>,
) -> impl Responder {
    // TODO: Implement actual withdrawal
    HttpResponse::Ok().json(serde_json::json!({
        "success": false,
        "message": "Withdrawal functionality coming soon"
    }))
}

/// POST /api/yield/positions/harvest
pub async fn harvest_rewards(
    pool: web::Data<DbPool>,
    body: web::Json<HarvestRequest>,
) -> impl Responder {
    // TODO: Implement reward harvesting
    HttpResponse::Ok().json(serde_json::json!({
        "success": false,
        "message": "Harvest functionality coming soon"
    }))
}

/// POST /api/yield/optimize/allocate
pub async fn optimize_allocation(
    pool: web::Data<DbPool>,
    body: web::Json<OptimizeRequest>,
) -> impl Responder {
    // TODO: Implement AI-powered optimization using regime detection + risk scoring
    
    // For now, return top 3 pools with simple allocation
    let top_pools = sqlx::query_as::<_, (String, String, String, f64, i32)>(
        "SELECT p.id, p.pool_name, pr.name, p.current_apy, pr.risk_score
         FROM yield_pools p
         JOIN yield_protocols pr ON p.protocol_id = pr.id
         WHERE p.active = true AND pr.enabled = true
         ORDER BY p.current_apy DESC
         LIMIT 3"
    )
    .fetch_all(pool.get_ref())
    .await;

    match top_pools {
        Ok(pools) => {
            let total_amount = body.total_amount_usd;
            let allocations: Vec<AllocationRecommendation> = pools.iter().enumerate().map(|(i, p)| {
                let percentage = match i {
                    0 => 50.0,
                    1 => 30.0,
                    _ => 20.0,
                };
                AllocationRecommendation {
                    pool_id: p.0.clone(),
                    pool_name: p.1.clone(),
                    protocol: p.2.clone(),
                    amount_usd: total_amount * (percentage / 100.0),
                    percentage,
                    apy: p.3,
                    reason: format!("High APY ({}%) with risk score {}", p.3, p.4),
                }
            }).collect();

            let expected_apy = allocations.iter()
                .map(|a| a.apy * (a.percentage / 100.0))
                .sum();

            HttpResponse::Ok().json(OptimizeResponse {
                allocations,
                expected_apy,
                risk_score: 5.0, // TODO: Calculate from AI risk engine
            })
        }
        Err(e) => {
            eprintln!("Failed to optimize: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to generate allocation"
            }))
        }
    }
}

// ============ Route Configuration ============

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/yield")
            .route("/protocols", web::get().to(get_protocols))
            .route("/pools/top", web::get().to(get_top_pools))
            .route("/positions", web::get().to(get_positions))
            .route("/rotations", web::get().to(get_rotations))
            .route("/portfolio", web::get().to(get_portfolio_summary))
            .route("/rotations/execute", web::post().to(execute_rotation))
            .route("/positions/deposit", web::post().to(deposit_position))
            .route("/positions/withdraw", web::post().to(withdraw_position))
            .route("/positions/harvest", web::post().to(harvest_rewards))
            .route("/optimize/allocate", web::post().to(optimize_allocation))
    );
}
