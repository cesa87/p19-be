use actix_web::{web, HttpResponse, Result};

use crate::config::Config;
use crate::error::AppError;
use crate::engine::optimizer::{self, OptimizationConfig};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/backtest/optimize")
            .route("", web::post().to(run_optimization))
            .route("/quick", web::get().to(run_quick_optimization))
    );
}

/// POST /api/backtest/optimize
/// Run a full parameter optimization sweep.
/// Accepts an optional OptimizationConfig body; uses defaults if empty.
async fn run_optimization(
    config: web::Data<Config>,
    body: Option<web::Json<OptimizationConfig>>,
) -> Result<HttpResponse, AppError> {
    let opt_config = body
        .map(|b| b.into_inner())
        .unwrap_or_default();

    tracing::info!("Optimizer: starting full run ({} estimated combos)", opt_config.total_combinations());

    let report = optimizer::run_optimization(config.get_ref(), opt_config).await?;

    Ok(HttpResponse::Ok().json(report))
}

/// GET /api/backtest/optimize/quick
/// Run a quick scan with reduced parameter space for fast feedback.
async fn run_quick_optimization(
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    let opt_config = OptimizationConfig::quick();

    tracing::info!("Optimizer: starting quick scan ({} estimated combos)", opt_config.total_combinations());

    let report = optimizer::run_optimization(config.get_ref(), opt_config).await?;

    Ok(HttpResponse::Ok().json(report))
}
