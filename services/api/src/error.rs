use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Error)]
pub enum AppError {
    #[error("Configuration invalid")]
    Configuration(#[from] config::ConfigError),
    #[error("Database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("I/O operation failed")]
    Io(#[from] std::io::Error),
    #[error("Operation failed: {operation}")]
    Context {
        operation: &'static str,
        #[source]
        source: Box<Self>,
    },

    #[error("Invalid username or password")]
    InvalidCredentials,

    #[error("Invalid or expired token")]
    InvalidToken,

    #[error("Record Not found")]
    NotFound,

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Your account is forbidden to make this action")]
    Forbidden,

    #[error("Database error")]
    Database(sqlx::Error),

    #[error("Username already exists")]
    Conflict,

    #[error("Password verification error")]
    PasswordHash(#[from] argon2::password_hash::Error),

    #[error("Token generation failed")]
    TokenCreation(#[from] jsonwebtoken::errors::Error),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

/// Emit a failure once when it will not be converted to an HTTP response.
pub fn log_background_error(operation: &'static str, error: &AppError) {
    let operation = if let AppError::Context { operation, .. } = error {
        operation
    } else {
        operation
    };
    emit(operation, error);
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (operation, error) = self.into_root();
        emit(operation, &error);
        error.response_without_logging()
    }
}

impl AppError {
    #[must_use]
    pub fn context(self, operation: &'static str) -> Self {
        Self::Context {
            operation,
            source: Box::new(self),
        }
    }
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppError")
            .field("kind", &self.category())
            .finish()
    }
}

impl AppError {
    fn into_root(self) -> (&'static str, Self) {
        let mut operation = "HTTP request";
        let mut error = self;
        while let Self::Context {
            operation: label,
            source,
        } = error
        {
            operation = label;
            error = *source;
        }
        (operation, error)
    }
    fn root(&self) -> &Self {
        match self {
            Self::Context { source, .. } => source.root(),
            other => other,
        }
    }
    fn category(&self) -> &'static str {
        match self.root() {
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidToken => "invalid_token",
            Self::NotFound => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden => "forbidden",
            Self::Conflict => "conflict",
            Self::Database(_) => "database",
            Self::PasswordHash(_) => "password_hash",
            Self::TokenCreation(_) => "token_creation",
            Self::Internal(_) => "internal",
            Self::Configuration(_) => "configuration",
            Self::Migration(_) => "migration",
            Self::Io(_) => "io",
            Self::Context { .. } => unreachable!(),
        }
    }
    fn response_without_logging(self) -> Response {
        match self {
            AppError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": self.to_string() })),
            )
                .into_response(),
            AppError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
                Json(json!({ "error": self.to_string() })),
            )
                .into_response(),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": self.to_string() })),
            )
                .into_response(),
            AppError::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
            }
            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": self.to_string() })),
            )
                .into_response(),
            AppError::Conflict => (
                StatusCode::CONFLICT,
                Json(json!({"error": self.to_string()})),
            )
                .into_response(),
            AppError::Database(_)
            | AppError::PasswordHash(_)
            | AppError::TokenCreation(_)
            | AppError::Internal(_)
            | AppError::Configuration(_)
            | AppError::Migration(_)
            | AppError::Io(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"Internal server error"})),
            )
                .into_response(),
            AppError::Context { .. } => unreachable!("context removed at handling boundary"),
        }
    }
}

fn emit(operation: &'static str, error: &AppError) {
    match error.root() {
        AppError::InvalidCredentials
        | AppError::InvalidToken
        | AppError::NotFound
        | AppError::BadRequest(_)
        | AppError::Forbidden
        | AppError::Conflict => {
            tracing::warn!(operation, kind = error.category(), "API request rejected");
        }
        _ => tracing::error!(operation, kind = error.category(), "API operation failed"),
    }
}
impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db) = error
            && db.is_unique_violation()
            && db.constraint() == Some("admin_username_key")
        {
            return Self::Conflict;
        }
        Self::Database(error)
    }
}

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;
