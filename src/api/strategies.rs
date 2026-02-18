use actix_web::{web, HttpResponse, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{Strategy, CreateStrategyRequest, UpdateStrategyRequest, StrategyResponse};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/strategies")
            .route("", web::get().to(list_strategies))
            .route("", web::post().to(create_strategy))
            .route("/{id}", web::get().to(get_strategy))
            .route("/{id}", web::put().to(update_strategy))
            .route("/{id}", web::delete().to(delete_strategy))
    );
}

async fn list_strategies(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    // TODO: Filter by user_id from JWT token when auth is implemented
    // For now, return all strategies
    let strategies = sqlx::query_as::<_, Strategy>(
        "SELECT * FROM strategies ORDER BY created_at DESC"
    )
    .fetch_all(pool.get_ref())
    .await?;

    let response: Vec<StrategyResponse> = strategies.into_iter().map(|s| s.into()).collect();
    
    Ok(HttpResponse::Ok().json(response))
}

async fn create_strategy(
    pool: web::Data<DbPool>,
    body: web::Json<CreateStrategyRequest>,
) -> Result<HttpResponse, AppError> {
    // TODO: Get user_id from JWT token
    // Using default user until auth is implemented
    let user_id = Uuid::parse_str("a0000000-0000-0000-0000-000000000001").unwrap();
    
    let now = Utc::now();
    let strategy = sqlx::query_as::<_, Strategy>(
        r#"
        INSERT INTO strategies (id, user_id, name, strategy_type, params, risk, custom_rules, is_active, mrate_category, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $9, $9)
        RETURNING *
        "#
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&body.name)
    .bind(body.strategy_type.to_string())
    .bind(&body.params)
    .bind(serde_json::to_value(&body.risk).unwrap())
    .bind(&body.custom_rules)
    .bind(&body.mrate_category)
    .bind(now)
    .fetch_one(pool.get_ref())
    .await?;

    let response: StrategyResponse = strategy.into();
    
    Ok(HttpResponse::Created().json(response))
}

async fn get_strategy(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    let strategy = sqlx::query_as::<_, Strategy>(
        "SELECT * FROM strategies WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Strategy not found".to_string()))?;

    let response: StrategyResponse = strategy.into();
    
    Ok(HttpResponse::Ok().json(response))
}

async fn update_strategy(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateStrategyRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    // First, fetch existing strategy
    let existing = sqlx::query_as::<_, Strategy>(
        "SELECT * FROM strategies WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::NotFound("Strategy not found".to_string()))?;

    // Update with new values or keep existing
    let name = body.name.clone().unwrap_or(existing.name);
    let strategy_type = body.strategy_type.as_ref()
        .map(|t| t.to_string())
        .unwrap_or(existing.strategy_type);
    let params = body.params.clone().unwrap_or(existing.params);
    let risk = body.risk.as_ref()
        .map(|r| serde_json::to_value(r).unwrap())
        .unwrap_or(existing.risk);
    let custom_rules = if body.custom_rules.is_some() {
        body.custom_rules.clone()
    } else {
        existing.custom_rules
    };
    let is_active = body.is_active.unwrap_or(existing.is_active);
    
    // mrate_category: Frontend always sends this field (either with value or null)
    // So we should always use what was sent, allowing users to clear it
    let mrate_category = body.mrate_category.clone();

    let strategy = sqlx::query_as::<_, Strategy>(
        r#"
        UPDATE strategies 
        SET name = $1, strategy_type = $2, params = $3, risk = $4, custom_rules = $5, is_active = $6, mrate_category = $7, updated_at = $8
        WHERE id = $9
        RETURNING *
        "#
    )
    .bind(&name)
    .bind(&strategy_type)
    .bind(&params)
    .bind(&risk)
    .bind(&custom_rules)
    .bind(is_active)
    .bind(&mrate_category)
    .bind(Utc::now())
    .bind(id)
    .fetch_one(pool.get_ref())
    .await?;

    let response: StrategyResponse = strategy.into();
    
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_strategy(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    
    let result = sqlx::query("DELETE FROM strategies WHERE id = $1")
        .bind(id)
        .execute(pool.get_ref())
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Strategy not found".to_string()));
    }
    
    Ok(HttpResponse::NoContent().finish())
}
