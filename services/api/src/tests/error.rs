#![allow(clippy::unwrap_used)]
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
