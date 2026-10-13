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

    #[error("Failure in {function}")]
    Origin {
        function: &'static str,
        #[source]
        source: Box<Self>,
    },

    #[error("Invalid username or password")]
    InvalidCredentials,

    #[error("Invalid or expired token")]
    InvalidToken,

    #[error("Invalid or expired token")]
    TokenValidation(#[source] jsonwebtoken::errors::Error),

    #[error("Record Not found")]
    NotFound,

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Your account is forbidden to make this action")]
    Forbidden,

    #[error("Database error")]
    Database(sqlx::Error),

    #[error("Username already exists")]
    Conflict(#[source] sqlx::Error),

    #[error("Password verification error")]
    PasswordHash(#[from] argon2::password_hash::Error),

    #[error("Token generation failed")]
    TokenCreation(#[from] jsonwebtoken::errors::Error),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

/// Emit a failure once when it will not be converted to an HTTP response.
pub fn log_background_error(operation: &'static str, error: &AppError) {
    emit(operation, error);
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        emit("HTTP request", &self);
        self.into_root().response_without_logging()
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

    #[must_use]
    pub fn at(self, function: &'static str) -> Self {
        if self.origin().is_some() {
            self
        } else {
            Self::Origin {
                function,
                source: Box::new(self),
            }
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
    fn into_root(self) -> Self {
        match self {
            Self::Context { source, .. } | Self::Origin { source, .. } => source.into_root(),
            other => other,
        }
    }

    fn root(&self) -> &Self {
        match self {
            Self::Context { source, .. } | Self::Origin { source, .. } => source.root(),
            other => other,
        }
    }

    fn origin(&self) -> Option<&'static str> {
        match self {
            Self::Origin { function, .. } => Some(function),
            Self::Context { source, .. } => source.origin(),
            _ => None,
        }
    }

    fn operation(&self, fallback: &'static str) -> &'static str {
        match self {
            Self::Context { operation, source } => source.operation(operation),
            Self::Origin { source, .. } => source.operation(fallback),
            _ => fallback,
        }
    }

    fn category(&self) -> &'static str {
        match self.root() {
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidToken | Self::TokenValidation(_) => "invalid_token",
            Self::NotFound => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Forbidden => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::Database(_) => "database",
            Self::PasswordHash(_) => "password_hash",
            Self::TokenCreation(_) => "token_creation",
            Self::Internal(_) => "internal",
            Self::Configuration(_) => "configuration",
            Self::Migration(_) => "migration",
            Self::Io(_) => "io",
            Self::Context { .. } | Self::Origin { .. } => unreachable!(),
        }
    }
    fn response_without_logging(self) -> Response {
        match self {
            AppError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": self.to_string() })),
            )
                .into_response(),
            AppError::InvalidToken | AppError::TokenValidation(_) => (
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
            AppError::Conflict(_) => (
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
            AppError::Context { .. } | AppError::Origin { .. } => {
                unreachable!("context removed at handling boundary")
            }
        }
    }
}

fn emit(operation: &'static str, error: &AppError) {
    let operation = error.operation(operation);
    let origin = error.origin().unwrap_or("unknown");
    match error.root() {
        AppError::InvalidCredentials
        | AppError::InvalidToken
        | AppError::TokenValidation(_)
        | AppError::NotFound
        | AppError::BadRequest(_)
        | AppError::Forbidden
        | AppError::Conflict(_) => {
            tracing::warn!(
                operation,
                origin,
                kind = error.category(),
                "API request rejected"
            );
        }
        _ => tracing::error!(
            operation,
            origin,
            kind = error.category(),
            "API operation failed"
        ),
    }
    tracing::debug!(operation, origin, kind = error.category(), error = ?Diagnostic(error), "API failure diagnostics");
}
struct Diagnostic<'a>(&'a AppError);

impl std::fmt::Debug for Diagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            AppError::Context { operation, source } => f
                .debug_struct("Context")
                .field("operation", operation)
                .field("source", &Diagnostic(source))
                .finish(),
            AppError::Origin { source, .. } => Diagnostic(source).fmt(f),
            AppError::Configuration(error) => std::fmt::Debug::fmt(error, f),
            AppError::Migration(error) => std::fmt::Debug::fmt(error, f),
            AppError::Io(error) => std::fmt::Debug::fmt(error, f),
            AppError::Database(error) | AppError::Conflict(error) => std::fmt::Debug::fmt(error, f),
            AppError::PasswordHash(error) => std::fmt::Debug::fmt(error, f),
            AppError::TokenCreation(error) | AppError::TokenValidation(error) => {
                std::fmt::Debug::fmt(error, f)
            }
            AppError::Internal(error) => std::fmt::Debug::fmt(error, f),
            error => std::fmt::Display::fmt(error, f),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db) = error
            && db.is_unique_violation()
            && db.constraint() == Some("admin_username_key")
        {
            return Self::Conflict(error);
        }
        Self::Database(error)
    }
}

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;
