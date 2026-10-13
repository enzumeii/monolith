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

    #[error("Service temporarily unavailable")]
    Unavailable,

    #[error("Password verification error")]
    PasswordHash(#[from] argon2::password_hash::Error),

    #[error("Token generation failed")]
    TokenCreation(#[from] jsonwebtoken::errors::Error),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
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
            Self::Unavailable => "unavailable",
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
            AppError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                [("retry-after", "1")],
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

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppError")
            .field("kind", &self.category())
            .finish()
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
/// Emit a failure once when it will not be converted to an HTTP response.
pub fn log_background_error(operation: &'static str, error: &AppError) {
    let operation = if let AppError::Context { operation, .. } = error {
        operation
    } else {
        operation
    };
    emit(operation, error);
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn database_error_hides_internal_detail_behind_generic_message() {
        let sql_error = sqlx::Error::RowNotFound;
        let response = AppError::Database(sql_error).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Internal server error");
        assert!(!body.to_string().contains("RowNotFound"));
    }

    #[tokio::test]
    async fn password_hash_error_hides_internal_detail_behind_generic_message() {
        let hash_error = argon2::password_hash::Error::Password;
        let response = AppError::PasswordHash(hash_error).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn token_creation_error_hides_internal_detail_behind_generic_message() {
        let jwt_error: jsonwebtoken::errors::Error =
            jsonwebtoken::errors::ErrorKind::InvalidToken.into();
        let response = AppError::TokenCreation(jwt_error).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn internal_error_hides_anyhow_detail_behind_generic_message() {
        let secret_detail = anyhow::anyhow!("database password is topsecret123");
        let response = AppError::Internal(secret_detail).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Internal server error");
        assert!(!body.to_string().contains("topsecret123"));
    }

    #[tokio::test]
    async fn bad_request_surfaces_its_own_message() {
        let response = AppError::BadRequest("page must be positive".into()).into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_json(response).await;
        assert_eq!(body["error"], "page must be positive");
    }

    #[tokio::test]
    async fn invalid_token_response_includes_www_authenticate_header() {
        let response = AppError::InvalidToken.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }

    #[tokio::test]
    async fn not_found_and_forbidden_map_to_expected_statuses() {
        assert_eq!(
            AppError::NotFound.into_response().status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::Forbidden.into_response().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::InvalidCredentials.into_response().status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };
    struct Capture(Arc<Mutex<Vec<u8>>>, std::thread::ThreadId);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if std::thread::current().id() == self.1 {
                self.0.lock().expect("capture").extend_from_slice(bytes);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn failure_is_logged_once_without_secret_details() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = bytes.clone();
        let owner = std::thread::current().id();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(move || Capture(output.clone(), owner))
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("test subscriber");
        {
            let error = AppError::from(anyhow::anyhow!("private-password-value"));
            assert_eq!(bytes.lock().expect("capture").len(), 0);
            let response = error.context("capture regression").into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }
        let log = String::from_utf8(bytes.lock().expect("capture").clone()).expect("UTF-8");
        assert_eq!(
            log.lines()
                .filter(|line| line.contains("capture regression"))
                .count(),
            1
        );
        assert!(!log.contains("private-password-value"));
    }
}
