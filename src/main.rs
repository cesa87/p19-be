use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod analytics;
mod api;
mod bot;
mod broker;
mod config;
mod db;
mod engine;
mod error;
mod feeds;
mod macro_sentiment;
mod models;
mod mrate;
mod news;
mod risk;
mod services;
mod sniper;
mod telegram;

use config::Config;
use bot::runner::BotManager;
use mrate::{MrateScheduler, create_mrate_state, create_mrate_engine};
use sniper::wallet::create_wallet_store;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,sqlx=warn".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("Failed to load configuration");
    let config = web::Data::new(config);

    // Initialize database pool
    let db_pool = db::create_pool(&config.database_url)
        .await
        .expect("Failed to create database pool");
    let db_pool_data = web::Data::new(db_pool.clone());
    
    // Initialize MRATE state and engine (shared between BotManager, scheduler, and API)
    let mrate_state = create_mrate_state();
    let mrate_state_data = web::Data::new(mrate_state.clone());
    let mrate_engine = create_mrate_engine();
    let mrate_engine_data = web::Data::new(mrate_engine.clone());
    
    // Initialize bot manager with MRATE state
    let config_inner = config.get_ref().clone();
    let bot_manager = BotManager::new(db_pool.clone(), config_inner.clone(), mrate_state.clone()).await;
    
    // Auto-restart bots that were active before backend restart
    let restarted_count = bot_manager.auto_restart_active_bots().await;
    if restarted_count > 0 {
        info!("🤖 Auto-restarted {} bot(s) from previous session", restarted_count);
    }
    
    // Get risk engine from bot manager to share with API
    let risk_engine = bot_manager.risk_engine();
    let risk_engine_data = web::Data::new(risk_engine);
    
    let bot_manager = web::Data::new(bot_manager);
    
    // Initialize wallet session store for sniper
    let wallet_store = create_wallet_store();
    let wallet_store_data = web::Data::new(wallet_store);
    
    // Start MRATE scheduler in background
    let mrate_scheduler = MrateScheduler::new(
        mrate_engine,
        mrate_state,
        db_pool,
        config_inner,
        true,  // persist_history - save MRATE snapshots for analysis
    );
    tokio::spawn(async move {
        mrate_scheduler.run().await;
    });

    info!("Starting Aureum Backend on {}:{}", config.host, config.port);

    let host = config.host.clone();
    let port = config.port;

    HttpServer::new(move || {
        // Configure CORS
        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")
            .allowed_origin("http://localhost:5174")
            .allowed_origin("http://localhost:3000")
            .allowed_origin("https://app.project-bubblegum.shop")
            .allowed_origin("https://project-bubblegum.shop")
            .allowed_origin("https://d1vf6eoz2xzfi8.cloudfront.net")
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::CONTENT_TYPE,
            ])
            .supports_credentials()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(middleware::Logger::default())
            .app_data(config.clone())
            .app_data(db_pool_data.clone())
            .app_data(bot_manager.clone())
            .app_data(mrate_state_data.clone())
            .app_data(mrate_engine_data.clone())
            .app_data(risk_engine_data.clone())
            .app_data(wallet_store_data.clone())
            // Health check
            .route("/health", web::get().to(health_check))
            // API routes
            .service(
                web::scope("/api")
                    .configure(api::ai::configure_ai_routes)
                    .configure(api::analytics::configure)
                    .configure(api::auth::configure)
                    .configure(api::strategies::configure)
                    .configure(api::optimizer::configure)
                    .configure(api::backtest::configure)
                    .configure(api::trading::configure)
                    .configure(api::market_data::configure)
                    .configure(api::copy_trading::configure)
                    .configure(api::bots::configure)
                    .configure(api::news::configure)
                    .configure(api::macro_sentiment::configure)
                    .configure(api::mrate::configure)
                    .configure(api::orchestrator::configure)
                    .configure(api::risk::configure)
                    .configure(api::sniper::configure)
                    .configure(api::yield_farming::configure)
            )
    })
    .bind((host, port))?
    .run()
    .await
}

async fn health_check() -> &'static str {
    "OK"
}
