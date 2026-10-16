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
    use std::{cell::RefCell, io::Write, sync::Once};
    use tracing_subscriber::{
        EnvFilter, Layer,
        filter::{LevelFilter, dynamic_filter_fn},
        layer::SubscriberExt,
        util::SubscriberInitExt,
    };

    thread_local! {
        static OUTPUT: RefCell<Option<(EnvFilter, Vec<u8>)>> = const { RefCell::new(None) };
    }

    struct Capture;
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            OUTPUT.with(|output| {
                if let Some((_, buffer)) = &mut *output.borrow_mut() {
                    buffer.extend_from_slice(bytes);
                }
            });
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture(filter: &str, action: impl FnOnce()) -> String {
        // A stable subscriber avoids global callsite-cache races between test filters.
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let filter = dynamic_filter_fn(|metadata, context| {
                OUTPUT.with(|output| {
                    output
                        .borrow()
                        .as_ref()
                        .is_some_and(|(filter, _)| filter.enabled(metadata, context.clone()))
                })
            })
            .with_max_level_hint(LevelFilter::DEBUG);
            let layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .without_time()
                .with_writer(|| Capture)
                .with_filter(filter);
            tracing_subscriber::registry().with(layer).init();
        });
        OUTPUT.with(|output| *output.borrow_mut() = Some((EnvFilter::new(filter), Vec::new())));
        action();
        OUTPUT.with(|output| {
            let (_, bytes) = output.borrow_mut().take().expect("capture");
            String::from_utf8(bytes).expect("UTF-8")
        })
    }

    #[test]
    fn info_logs_one_summary_without_diagnostics() {
        let log = capture("info", || {
            let error = AppError::from(anyhow::anyhow!("private-password-value"));
            assert!(!format!("{error:?}").contains("private-password-value"));
            let response = error.context("capture regression").into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
        assert_eq!(log.lines().count(), 1);
        assert!(log.contains("capture regression"));
        assert!(!log.contains("private-password-value"));
    }

    #[test]
    fn debug_logs_underlying_error_and_nested_context() {
        let log = capture("info,api=debug", || {
            let cause = anyhow::anyhow!("diagnostic root cause").context("driver context");
            let error = AppError::from(cause)
                .context("query operation")
                .context("HTTP operation");
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            tracing::debug!("application debug event");
        });
        assert_eq!(
            log.lines()
                .filter(|line| line.contains("API operation failed"))
                .count(),
            1
        );
        assert!(log.contains("diagnostic root cause"), "{log}");
        assert!(log.contains("driver context"), "{log}");
        assert!(log.contains("query operation"), "{log}");
        assert!(log.contains("HTTP operation"), "{log}");
        assert!(log.contains("application debug event"), "{log}");
    }

    #[test]
    fn password_failure_keeps_its_function_origin_at_http_boundary() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime
            .block_on(crate::utils::password::verify(
                "unused-password".into(),
                "malformed".into(),
            ))
            .unwrap_err();
        let log = capture("info", || {
            let response = error.context("login request").into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
        assert!(
            log.contains("origin=\"api::utils::password::verify\""),
            "{log}"
        );
        assert!(!log.contains("unused-password"));
        assert!(!log.contains("malformed"));
    }

    #[test]
    fn background_failure_logs_original_database_diagnostic() {
        let log = capture("info,api=debug", || {
            let error = AppError::Database(sqlx::Error::RowNotFound).context("retention query");
            log_background_error("refresh token retention", &error);
        });
        assert_eq!(
            log.lines()
                .filter(|line| line.contains("API operation failed"))
                .count(),
            1
        );
        assert!(log.contains("RowNotFound"), "{log}");
        assert!(log.contains("retention query"), "{log}");
    }

    #[test]
    fn repository_origin_survives_service_and_handler_propagation() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime.block_on(async {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap();
            pool.close().await;
            crate::services::admin::get(&pool, 1).await.unwrap_err()
        });
        let log = capture("info,api=debug", || {
            let response = error
                .at("api::handlers::admin::get")
                .context("get admin")
                .into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
        assert!(
            log.contains("origin=\"api::repositories::admin::find_by_id\""),
            "{log}"
        );
        assert!(log.contains("PoolClosed"), "{log}");
    }

    #[test]
    fn token_rejection_keeps_driver_diagnostic_and_bearer_header() {
        let error =
            crate::middleware::token::validate_token("invalid-token", &[0; 32]).unwrap_err();
        let log = capture("info,api=debug", || {
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()["www-authenticate"], "Bearer");
        });
        assert!(
            log.contains("origin=\"api::middleware::token::validate_token\""),
            "{log}"
        );
        assert!(log.contains("InvalidToken"), "{log}");
        assert!(!log.contains("invalid-token"), "{log}");
    }

    #[test]
    fn attaching_origin_and_context_does_not_emit_events() {
        let log = capture("info,api=debug", || {
            let _error = AppError::from(anyhow::anyhow!("root cause"))
                .at("api::services::auth::login")
                .context("login");
        });
        assert!(log.is_empty(), "{log}");
    }

    #[test]
    fn logout_pool_failure_identifies_the_service_function() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let error = runtime.block_on(async {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap();
            pool.close().await;
            let actor = crate::models::AuthenticatedAdmin {
                id: 1,
                role: domain::dto::AdminRole::Admin,
            };
            crate::services::auth::logout(
                &pool,
                &actor,
                domain::dto::RefreshRequest {
                    refresh_token: "unused-token".into(),
                },
            )
            .await
            .unwrap_err()
        });
        let log = capture("info", || {
            let _ = error.into_response();
        });
        assert!(
            log.contains("origin=\"api::services::auth::logout\""),
            "{log}"
        );
        assert!(!log.contains("unused-token"), "{log}");
    }

    #[test]
    #[ignore = "requires the configured Docker PostgreSQL; run just test"]
    fn username_conflict_keeps_postgres_diagnostics_and_generic_response() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let sql_error = runtime.block_on(async {
            let url = std::env::var("E2E_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TEMP TABLE admin (username TEXT CONSTRAINT admin_username_key UNIQUE)",
            )
            .execute(&pool)
            .await
            .unwrap();
            let username = format!("error_test_{}", uuid::Uuid::new_v4());
            sqlx::query("INSERT INTO admin (username) VALUES ($1)")
                .bind(&username)
                .execute(&pool)
                .await
                .unwrap();
            let error = sqlx::query("INSERT INTO admin (username) VALUES ($1)")
                .bind(&username)
                .execute(&pool)
                .await
                .unwrap_err();
            pool.close().await;
            error
        });
        let error = AppError::from(sql_error).at("api::repositories::admin::add");
        let log = capture("info,api=debug", || {
            let response = error.into_response();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            let body = runtime.block_on(body_json(response));
            assert_eq!(body["error"], "Username already exists");
            assert!(!body.to_string().contains("23505"));
        });
        assert_eq!(
            log.lines()
                .filter(|line| line.contains("API request rejected"))
                .count(),
            1
        );
        assert!(log.contains("admin_username_key"), "{log}");
        assert!(log.contains("23505"), "{log}");
    }
    #[test]
    fn upstream_errors_keep_origin_without_logging_customer_payloads() {
        for filter in ["info", "info,api=debug"] {
            let log = capture(filter, || {
                let error: AppError = crate::client::ClientError::Graphql {
                    errors: vec![crate::client::GraphqlError {
                        message: "private-customer-token".into(),
                        path: vec![],
                        extensions: std::collections::BTreeMap::new(),
                    }],
                    extensions: serde_json::json!({"payload":"private-customer-token"}),
                }
                .into();
                assert_eq!(
                    error.into_response().status(),
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            });
            assert!(log.contains("origin=\"api::client\""), "{log}");
            assert!(!log.contains("private-customer-token"), "{log}");
            assert_eq!(
                log.lines()
                    .filter(|line| line.contains("API operation failed"))
                    .count(),
                1
            );
        }
        let error: AppError = crate::client::ClientError::Decode {
            origin: "api::client::decode",
            source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
        }
        .into();
        assert_eq!(
            error.at("api::services::caller").origin(),
            Some("api::client::decode")
        );
    }
}

#[tokio::test]
async fn upstream_error_converts_with_source_and_safe_public_response() {
    use crate::client::ClientError;
    fn failing_operation() -> Result<(), AppError> {
        Err(ClientError::Graphql {
            errors: vec![],
            extensions: serde_json::json!({"private":"customer-token"}),
        })?;
        Ok(())
    }
    let error = failing_operation().unwrap_err();
    assert_eq!(error.origin(), Some("api::client"));
    match error.root() {
        AppError::Internal(source) => assert!(source.downcast_ref::<ClientError>().is_some()),
        _ => panic!("typed client source not retained"),
    }
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({"error":"Internal server error"})
    );
}
