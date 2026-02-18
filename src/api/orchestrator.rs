//! Orchestrator API endpoints
//!
//! Provides endpoints for the MRATE Strategy Orchestrator

use actix_web::{web, HttpResponse, Result};

use crate::db::DbPool;
use crate::error::AppError;
use crate::mrate::orchestrator::Orchestrator;
use crate::mrate::{get_current_mrate, default_mrate_output, MrateState};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/orchestrator")
            .route("/recommendations", web::get().to(get_recommendations))
            .route("/gaps", web::get().to(get_coverage_gaps))
    );
}

/// Get strategy recommendations based on current market conditions
async fn get_recommendations(
    pool: web::Data<DbPool>,
    mrate_state: web::Data<MrateState>,
) -> Result<HttpResponse, AppError> {
    // Get current MRATE (or default if not yet calculated)
    let mrate = get_current_mrate(&mrate_state).await
        .unwrap_or_else(default_mrate_output);
    
    // Create orchestrator and get recommendations
    let orchestrator = Orchestrator::new(pool.get_ref().clone());
    
    let response = orchestrator.get_recommendations(&mrate)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(response))
}

/// Get frequent coverage gaps for strategy development
async fn get_coverage_gaps(
    pool: web::Data<DbPool>,
) -> Result<HttpResponse, AppError> {
    let orchestrator = Orchestrator::new(pool.get_ref().clone());
    
    // Get gaps that have occurred at least 3 times
    let gaps = orchestrator.get_frequent_gaps(3)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    
    Ok(HttpResponse::Ok().json(gaps))
}
