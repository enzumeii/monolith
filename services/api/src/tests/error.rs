#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::ClientErrorKind;
use http_body_util::BodyExt;

fn assert_location(error: &AppError, line: u32) {
    assert_eq!(error.location.file(), file!());
    assert_eq!(error.location.line(), line);
    assert!(error.location.column() > 0);
}

#[test]
fn constructors_and_typed_conversions_capture_the_caller() {
    let line = line!() + 1;
    let error = AppError::new(StatusCode::BAD_REQUEST, "Invalid input");
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(sqlx::Error::RowNotFound);
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(std::io::Error::other("private-io"));
    assert_location(&error, line);
    let source: jsonwebtoken::errors::Error = jsonwebtoken::errors::ErrorKind::InvalidToken.into();
    let line = line!() + 1;
    let error = AppError::from(source);
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(config::ConfigError::Message("private-config".into()));
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(sqlx::migrate::MigrateError::VersionMissing(42));
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(argon2::password_hash::Error::Password);
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(crate::client::ClientError::new(ClientErrorKind::Protocol(
        "private-client",
    )));
    assert_location(&error, line);
    let line = line!() + 1;
    let error = AppError::from(anyhow::anyhow!("private-anyhow"));
    assert_location(&error, line);
}

#[tokio::test]
async fn question_mark_and_conversion_closures_capture_the_call_site() {
    async fn fail(expected_line: &mut u32) -> Result<(), AppError> {
        tokio::task::yield_now().await;
        *expected_line = line!() + 1;
        Err::<(), _>(std::io::Error::other("private-io"))?;
        Ok(())
    }
    let mut expected_line = 0;
    let error = fail(&mut expected_line).await.unwrap_err();
    assert_location(&error, expected_line);
    let line = line!() + 2;
    let error = Err::<(), _>(std::io::Error::other("private-io"))
        .map_err(|error| AppError::from(error).context("conversion callback"))
        .unwrap_err();
    assert_location(&error, line);
}

#[tokio::test]
async fn builders_and_propagation_preserve_location_without_exposing_it() {
    fn propagate(error: AppError) -> Result<(), AppError> {
        Err::<(), _>(error)?;
        Ok(())
    }
    let line = line!() + 1;
    let error = AppError::new(StatusCode::UNAUTHORIZED, "Not authorized");
    let error = error
        .with_source(std::io::Error::other("private-source"))
        .with_bearer_challenge()
        .context("first operation")
        .context("outer operation");
    let error = propagate(error).unwrap_err();
    assert_location(&error, line);
    assert_eq!(error.contexts, ["first operation", "outer operation"]);
    assert!(!format!("{error:?}").contains(file!()));
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[WWW_AUTHENTICATE], "Bearer");
    assert_eq!(
        body_json(response).await,
        serde_json::json!({"error":"Not authorized"})
    );
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn database_error_hides_internal_detail_behind_generic_message() {
    let sql_error = sqlx::Error::RowNotFound;
    let response = AppError::from(sql_error).into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "Internal server error");
    assert!(!body.to_string().contains("RowNotFound"));
}

#[tokio::test]
async fn password_hash_error_hides_internal_detail_behind_generic_message() {
    let hash_error = argon2::password_hash::Error::Password;
    let response = AppError::from(hash_error).into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "Internal server error");
}

#[tokio::test]
async fn token_creation_error_hides_internal_detail_behind_generic_message() {
    let jwt_error: jsonwebtoken::errors::Error =
        jsonwebtoken::errors::ErrorKind::InvalidToken.into();
    let response = AppError::from(jwt_error).into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "Internal server error");
}

#[tokio::test]
async fn internal_error_hides_anyhow_detail_behind_generic_message() {
    let secret_detail = anyhow::anyhow!("database password is topsecret123");
    let response = AppError::from(secret_detail).into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_json(response).await;
    assert_eq!(body["error"], "Internal server error");
    assert!(!body.to_string().contains("topsecret123"));
}

#[tokio::test]
async fn bad_request_surfaces_its_own_message() {
    let response = AppError::new(StatusCode::BAD_REQUEST, "page must be positive").into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    assert_eq!(body["error"], "page must be positive");
}

#[tokio::test]
async fn invalid_token_response_includes_www_authenticate_header() {
    let response = AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
        .with_bearer_challenge()
        .into_response();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn not_found_and_forbidden_map_to_expected_statuses() {
    assert_eq!(
        AppError::new(StatusCode::NOT_FOUND, "Shopify location not found")
            .into_response()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action"
        )
        .into_response()
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        AppError::new(StatusCode::UNAUTHORIZED, "Invalid username or password")
            .into_response()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn status_message_and_challenge_are_independent() {
    for (status, message, challenge) in [
        (StatusCode::BAD_REQUEST, "Invalid input", false),
        (
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
            false,
        ),
        (StatusCode::UNAUTHORIZED, "Not authorized", true),
        (StatusCode::FORBIDDEN, "Permission denied", false),
        (StatusCode::NOT_FOUND, "Shopify location not found", false),
        (StatusCode::CONFLICT, "Username already exists", false),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Shopify client unavailable",
            false,
        ),
        (StatusCode::BAD_GATEWAY, "Upstream request failed", false),
        (
            StatusCode::GATEWAY_TIMEOUT,
            "Upstream request failed",
            false,
        ),
    ] {
        let mut error = AppError::new(status, message.to_owned())
            .with_source(anyhow::anyhow!("private-source"));
        if challenge {
            error = error.with_bearer_challenge();
        }
        let debug = format!("{error:?}");
        assert!(!debug.contains(message));
        assert!(!debug.contains("private-source"));
        let response = error.into_response();
        assert_eq!(response.status(), status);
        assert_eq!(
            response
                .headers()
                .get(WWW_AUTHENTICATE)
                .map(|value| value.to_str().unwrap()),
            challenge.then_some("Bearer")
        );
        assert_eq!(
            body_json(response).await,
            serde_json::json!({"error":message})
        );
    }
}

#[tokio::test]
async fn unexpected_typed_causes_keep_private_sources() {
    for error in [
        AppError::from(config::ConfigError::Message("private-config".into())),
        AppError::from(sqlx::migrate::MigrateError::VersionMissing(42)),
        AppError::from(std::io::Error::other("private-io")),
    ] {
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.cause.is_some());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body_json(response).await,
            serde_json::json!({"error":"Internal server error"})
        );
    }
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
                .event_format(crate::logging::SourceFormat)
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
    fn report_failures_keep_private_diagnostics() {
        for filter in ["info", "info,api=debug"] {
            let log = capture(filter, || {
                let error = AppError::new(StatusCode::BAD_GATEWAY, "Upstream request failed")
                    .with_source(crate::client::ClientError::new(ClientErrorKind::Protocol(
                        "private-report-cause",
                    )));
                let response = error.into_response();
                assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
            });
            assert_eq!(
                log.contains("private-report-cause"),
                filter.ends_with("debug"),
                "{log}"
            );
            assert!(log.contains("client_origin=\"services/api/src/tests/error.rs:"));
        }
    }

    #[test]
    fn wrapped_and_shared_client_causes_keep_diagnostics_without_payloads() {
        use crate::client::ClientError;
        use std::sync::Arc;

        for filter in ["info", "info,api=debug"] {
            for shared in [false, true] {
                for wrapped in [false, true] {
                    let mut expected_client_origin = String::new();
                    let log = capture(filter, || {
                        let source = ClientError::new(ClientErrorKind::Graphql {
                            errors: vec![crate::client::GraphqlError {
                                message: "upstream-diagnostic".into(),
                                path: vec![],
                                extensions: std::collections::BTreeMap::from([(
                                    "private".into(),
                                    serde_json::json!("private-payload"),
                                )]),
                            }],
                            extensions: serde_json::json!({"private":"private-extension"}),
                        });
                        let origin = source.location();
                        expected_client_origin = origin.to_string();
                        let cause = if shared {
                            anyhow::Error::new(Arc::new(source))
                        } else {
                            anyhow::Error::new(source)
                        };
                        let cause = if wrapped {
                            cause.context("upstream context")
                        } else {
                            cause
                        };
                        let error = AppError::new(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "Shopify client unavailable",
                        )
                        .with_source(cause)
                        .context("report request");
                        assert_eq!(client_origin(&error), Some(origin));
                        assert_eq!(error.location.file(), file!());
                        assert_eq!(
                            error.into_response().status(),
                            StatusCode::SERVICE_UNAVAILABLE
                        );
                    });
                    assert_eq!(
                        log.contains("upstream-diagnostic"),
                        filter.ends_with("debug"),
                        "{log}"
                    );
                    assert_eq!(
                        log.contains("upstream context"),
                        wrapped && filter.ends_with("debug"),
                        "{log}"
                    );
                    assert!(!log.contains("private-payload"), "{log}");
                    assert!(!log.contains("private-extension"), "{log}");
                    assert!(log.contains("[503]"), "{log}");
                    assert!(
                        log.contains(&format!("client_origin=\"{expected_client_origin}\"")),
                        "{log}"
                    );
                }
            }
        }
    }

    #[test]
    fn failure_prefix_uses_original_location_and_status_without_duplicate_fields() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::BAD_REQUEST,
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let mut expected = String::new();
            let log = capture("info", || {
                let error = AppError::new(status, "private message");
                let (level, message) = if status.is_client_error() {
                    ("WARN", "API request rejected")
                } else {
                    ("ERROR", "API operation failed")
                };
                expected = format!(
                    "{level} {}: [{}] {message} operation=\"HTTP request\"",
                    error.location(),
                    status.as_u16()
                );
                let response = error.into_response();
                assert_eq!(response.status(), status);
            });
            assert!(log.contains(&expected), "{log}");
            assert_eq!(log.lines().count(), 1);
            for excluded in ["api::error:", "origin=", "status=", "private message"] {
                assert!(!log.contains(excluded), "{log}");
            }
        }
    }

    #[test]
    fn ordinary_events_retain_default_format_and_span_context() {
        let log = capture("info", || {
            let span = tracing::info_span!("request", request_id = 42);
            let _entered = span.enter();
            tracing::info!(count = 3, "ordinary event");
        });
        assert!(log.contains("request{request_id=42}"), "{log}");
        assert!(log.contains("api::error::tests::logging_tests:"), "{log}");
        assert!(log.contains("ordinary event count=3"), "{log}");
    }

    #[test]
    fn info_logs_one_summary_without_diagnostics() {
        let mut expected_origin = String::new();
        let log = capture("info", || {
            let error = AppError::from(anyhow::anyhow!("private-password-value"));
            expected_origin = format!("{}:", error.location());
            assert!(!format!("{error:?}").contains("private-password-value"));
            assert!(!format!("{error:?}").contains(error.location().file()));
            let response = error.context("capture regression").into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
        assert_eq!(log.lines().count(), 1);
        assert!(log.contains(&expected_origin), "{log}");
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
    fn password_failure_keeps_its_location_at_http_boundary() {
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
        assert!(log.contains("services/api/src/utils/password.rs:"), "{log}");
        assert!(!log.contains("unused-password"));
        assert!(!log.contains("malformed"));
    }

    #[test]
    fn background_failure_logs_original_database_diagnostic() {
        let mut expected_origin = String::new();
        let log = capture("info,api=debug", || {
            let error = AppError::from(sqlx::Error::RowNotFound).context("retention query");
            expected_origin = format!("{}:", error.location());
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
        assert!(log.contains(&expected_origin), "{log}");
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
            let response = error.context("get admin").into_response();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        });
        assert!(
            log.contains("services/api/src/repositories/admin.rs:"),
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
            log.contains("services/api/src/middleware/token.rs:"),
            "{log}"
        );
        assert!(log.contains("InvalidToken"), "{log}");
        assert!(!log.contains("invalid-token"), "{log}");
    }

    #[test]
    fn attaching_origin_and_context_does_not_emit_events() {
        let log = capture("info,api=debug", || {
            let _error = AppError::from(anyhow::anyhow!("root cause")).context("login");
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
        assert!(log.contains("services/api/src/services/auth.rs:"), "{log}");
        assert!(!log.contains("unused-token"), "{log}");
    }

    #[test]
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
        let error = AppError::from(sql_error);
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
    fn shopify_constructor_errors_identify_the_specific_client() {
        let source = crate::client::shopify::ShopifyClient::new(&crate::config::ShopifyConfig {
            admin_api_url: "invalid URL".into(),
            timeout_secs: 3,
            access_token: None,
        })
        .err()
        .unwrap();
        let location = source.location();
        assert_eq!(location.file(), "services/api/src/client/shopify.rs");
        let error: AppError = source.into();
        assert_eq!(client_origin(&error), Some(location));
    }
    fn client_error_cases() -> Vec<crate::client::ClientError> {
        use crate::client::{ClientError, GraphqlError, MutationUserError};
        vec![
            ClientErrorKind::Transport {
                source: reqwest::Client::new()
                    .get("invalid URL")
                    .build()
                    .unwrap_err(),
            },
            ClientErrorKind::Http {
                status: 503,
                request_id: Some("private-upstream-value".into()),
            },
            ClientErrorKind::Decode {
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            ClientErrorKind::Graphql {
                errors: vec![GraphqlError {
                    message: "private-upstream-value".into(),
                    path: vec![],
                    extensions: std::collections::BTreeMap::new(),
                }],
                extensions: serde_json::json!({"secret":"private-upstream-value"}),
            },
            ClientErrorKind::Mutation {
                errors: vec![MutationUserError {
                    field: Some(vec!["private-upstream-value".into()]),
                    message: "private-upstream-value".into(),
                }],
            },
            ClientErrorKind::Protocol("invalid page"),
            ClientErrorKind::InvalidInput("invalid argument"),
            ClientErrorKind::RetryExhausted,
        ]
        .into_iter()
        .map(|kind| ClientError::new(kind))
        .collect()
    }

    #[test]
    fn every_client_error_logs_once_at_either_handling_boundary() {
        for background in [false, true] {
            for filter in ["info", "info,api=debug"] {
                for wrapped in [false, true] {
                    for source in client_error_cases() {
                        let kind = std::mem::discriminant(source.kind());
                        let origin = source.location();
                        let mut converted = None;
                        let log = capture(filter, || {
                            let cause = anyhow::Error::new(source);
                            let cause = if wrapped {
                                cause.context("client cause context")
                            } else {
                                cause
                            };
                            converted = Some(AppError::from(cause).context("upstream operation"));
                        });
                        assert!(log.is_empty(), "{log}");
                        let error = converted.unwrap();
                        assert_eq!(client_origin(&error), Some(origin));
                        let typed = error
                            .cause
                            .as_ref()
                            .unwrap()
                            .chain()
                            .find_map(client_cause)
                            .unwrap();
                        assert_eq!(std::mem::discriminant(typed.kind()), kind);
                        let log = capture(filter, || {
                            if background {
                                log_background_error("upstream job", &error);
                            } else {
                                assert_eq!(
                                    error.into_response().status(),
                                    StatusCode::INTERNAL_SERVER_ERROR
                                );
                            }
                        });
                        assert_eq!(
                            log.lines().filter(|line| line.contains("ERROR")).count(),
                            1,
                            "{log}"
                        );
                        assert!(
                            log.contains(&format!("client_origin=\"{origin}\"")),
                            "{log}"
                        );
                        assert!(!log.contains("private-upstream-value"), "{log}");
                    }
                }
            }
        }
    }

    #[test]
    fn real_shopify_failures_log_the_client_at_existing_boundaries() {
        use crate::{
            client::shopify::ShopifyClient,
            client::tests::{Fixture, Reply},
            config::ShopifyConfig,
            shopify_models::OrdersByTagRequest,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for kind in ["constructor", "http", "decode", "protocol"] {
            for background in [false, true] {
                for filter in ["info", "info,api=debug"] {
                    let source = runtime.block_on(async {
                        if kind == "constructor" {
                            return ShopifyClient::new(&ShopifyConfig {
                                admin_api_url: "invalid URL".into(),
                                timeout_secs: 3,
                                access_token: None,
                            })
                            .err()
                            .unwrap();
                        }
                        let reply = match kind {
                            "http" => Reply::status(503),
                            "decode" => Reply {
                                status: 200,
                                body: "{".into(),
                                headers: vec![],
                            },
                            _ => Reply::json(serde_json::json!({"data":null})),
                        };
                        let fixture = Fixture::new(vec![reply]).await;
                        let client = ShopifyClient::new(&ShopifyConfig {
                            admin_api_url: fixture.url.to_string(),
                            timeout_secs: 3,
                            access_token: Some("private-upstream-value".into()),
                        })
                        .unwrap();
                        client
                            .orders_by_tag(
                                &OrdersByTagRequest {
                                    tags: vec!["private-upstream-value".into()],
                                },
                                &crate::shopify_models::PageRequest {
                                    first: 10,
                                    after: None,
                                },
                            )
                            .await
                            .err()
                            .unwrap()
                    });
                    let location = source.location();
                    assert_eq!(
                        location.file(),
                        if kind == "constructor" {
                            "services/api/src/client/shopify.rs"
                        } else {
                            "services/api/src/client.rs"
                        }
                    );
                    let log = capture(filter, || {
                        let error: AppError = source.into();
                        assert_eq!(client_origin(&error), Some(location));
                        if background {
                            log_background_error("Shopify job", &error);
                        } else {
                            let response = error.into_response();
                            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
                            assert_eq!(
                                runtime.block_on(body_json(response)),
                                serde_json::json!({"error":"Internal server error"})
                            );
                        }
                    });
                    assert_eq!(
                        log.lines().filter(|l| l.contains("ERROR")).count(),
                        1,
                        "{log}"
                    );
                    assert!(
                        log.contains(&format!("client_origin=\"{location}\"")),
                        "{log}"
                    );
                    assert!(!log.contains("private-upstream-value"), "{log}");
                }
            }
        }
    }

    #[test]
    fn upstream_errors_keep_origin_without_logging_customer_payloads() {
        for filter in ["info", "info,api=debug"] {
            let log = capture(filter, || {
                let error: AppError = crate::client::ClientError::new(ClientErrorKind::Graphql {
                    errors: vec![crate::client::GraphqlError {
                        message: "private-customer-token".into(),
                        path: vec![],
                        extensions: std::collections::BTreeMap::new(),
                    }],
                    extensions: serde_json::json!({"payload":"private-customer-token"}),
                })
                .into();
                assert_eq!(
                    error.into_response().status(),
                    StatusCode::INTERNAL_SERVER_ERROR
                );
            });
            assert!(
                log.contains("client_origin=\"services/api/src/tests/error.rs:"),
                "{log}"
            );
            assert!(!log.contains("private-customer-token"), "{log}");
            assert_eq!(
                log.lines()
                    .filter(|line| line.contains("API operation failed"))
                    .count(),
                1
            );
        }
        let source = crate::client::ClientError::from(
            serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
        );
        let location = source.location();
        let error: AppError = source.into();
        assert_eq!(client_origin(&error), Some(location));
    }
}

#[tokio::test]
async fn upstream_error_converts_with_source_and_safe_public_response() {
    use crate::client::ClientError;
    fn failing_operation() -> Result<(), AppError> {
        Err(ClientError::new(ClientErrorKind::Graphql {
            errors: vec![],
            extensions: serde_json::json!({"private":"customer-token"}),
        }))?;
        Ok(())
    }
    let error = failing_operation().unwrap_err();
    assert_eq!(client_origin(&error).unwrap().file(), file!());
    assert!(
        error
            .cause
            .as_ref()
            .unwrap()
            .downcast_ref::<ClientError>()
            .is_some()
    );
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({"error":"Internal server error"})
    );
}
