use actix_web::{HttpResponse, ResponseError};
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    InternalError(String),
    DatabaseError(String),
    ValidationError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            AppError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            AppError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            AppError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::NotFound(msg) => {
                HttpResponse::NotFound().json(serde_json::json!({ "error": msg }))
            }
            AppError::BadRequest(msg) => {
                HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }))
            }
            AppError::Unauthorized(msg) => {
                HttpResponse::Unauthorized().json(serde_json::json!({ "error": msg }))
            }
            AppError::InternalError(msg) => {
                HttpResponse::InternalServerError().json(serde_json::json!({ "error": msg }))
            }
            AppError::DatabaseError(msg) => {
                HttpResponse::InternalServerError().json(serde_json::json!({ "error": msg }))
            }
            AppError::ValidationError(msg) => {
                HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }))
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::DatabaseError(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::Unauthorized(err.to_string())
    }
}
