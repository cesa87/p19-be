use actix_web::{web, HttpResponse, Result};
use argon2::{password_hash::{SaltString, PasswordHasher, PasswordVerifier, rand_core::OsRng}, Argon2};
use chrono::{Utc, Duration};
use jsonwebtoken::{encode, Header, EncodingKey};
use uuid::Uuid;

use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{LoginRequest, RegisterRequest, AuthResponse, UserResponse, Claims, User};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/login", web::post().to(login))
            .route("/register", web::post().to(register))
    );
}

async fn login(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    // Find user by email
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = $1"
    )
    .bind(&body.email)
    .fetch_optional(pool.get_ref())
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Verify password
    let argon2 = Argon2::default();
    let parsed_hash = argon2::PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::InternalError("Failed to parse password hash".to_string()))?;
    
    argon2.verify_password(body.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;

    // Generate JWT
    let token = generate_token(&user, &config)?;

    Ok(HttpResponse::Ok().json(AuthResponse {
        token,
        user: user.into(),
    }))
}

async fn register(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, AppError> {
    // Check if user already exists
    let existing = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = $1"
    )
    .bind(&body.email)
    .fetch_optional(pool.get_ref())
    .await?;

    if existing.is_some() {
        return Err(AppError::BadRequest("Email already registered".to_string()));
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(body.password.as_bytes(), &salt)
        .map_err(|_| AppError::InternalError("Failed to hash password".to_string()))?
        .to_string();

    // Create user
    let user = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, email, password_hash, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $5)
        RETURNING *
        "#
    )
    .bind(Uuid::new_v4())
    .bind(&body.email)
    .bind(&password_hash)
    .bind(&body.name)
    .bind(Utc::now())
    .fetch_one(pool.get_ref())
    .await?;

    // Generate JWT
    let token = generate_token(&user, &config)?;

    Ok(HttpResponse::Created().json(AuthResponse {
        token,
        user: user.into(),
    }))
}

fn generate_token(user: &User, config: &Config) -> Result<String, AppError> {
    let expiry = Utc::now() + Duration::hours(config.jwt_expiry_hours);
    
    let claims = Claims {
        sub: user.id,
        email: user.email.clone(),
        exp: expiry.timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::InternalError(e.to_string()))
}
