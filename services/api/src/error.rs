use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Invalid username or password")]
    InvalidCredentials,

    #[error("Database error")]
    Database(#[from] sqlx::Error),

    #[error("Password verification error")]
    PasswordHash(#[from] argon2::password_hash::Error),

    #[error("Token generation failed")]
    TokenCreation(#[from] jsonwebtoken::errors::Error),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Database(ref err) => {
                tracing::error!(error = %err, "Database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            AppError::PasswordHash(ref err) => {
                tracing::error!(error = %err, "Password verification failed unexpectedly");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            AppError::TokenCreation(ref err) => {
                tracing::error!(error = %err, "JWT signing failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            AppError::Internal(ref err) => {
                tracing::error!(error = %err, "Unhandled application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };

        let body = Json(json!({ "error": message }));
        (status, body).into_response()
    }
}
