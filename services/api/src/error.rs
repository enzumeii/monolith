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
            AppError::Database(ref err) => {
                tracing::error!(error = %err, "Database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
            AppError::PasswordHash(ref err) => {
                tracing::error!(error = %err, "Password verification failed unexpectedly");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
            AppError::TokenCreation(ref err) => {
                tracing::error!(error = %err, "JWT signing failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
            AppError::Internal(ref err) => {
                tracing::error!(error = %err, "Unhandled application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
        }
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
