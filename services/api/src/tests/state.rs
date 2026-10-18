#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::StatusCode;
use serde_json::json;
fn settings(shopify: &serde_json::Value) -> Settings {
    serde_json::from_value(json!({"client":{"shopify":shopify},"browser":{"public_origin":"http://localhost:8084","insecure_local":true},"server":{"host":"localhost","port":3000},"database":{"url":"unused","max_connections":1,"min_connections":1},"jwt":{"secret":"1111111111111111111111111111111111111111111111111111111111111111"}})).unwrap()
}
#[tokio::test]
async fn state_reuses_shopify_client_and_preserves_optional_initialization_failures() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    let state = app_state(
        pool.clone(),
        settings(
            &json!({"timeout_secs":3,"admin_api_url":"http://localhost:9999/graphql","access_token":"fixture"}),
        ),
    )
    .unwrap();
    let cloned = state.clone();
    assert!(Arc::ptr_eq(
        state.shopify.as_ref().unwrap().as_ref().unwrap(),
        cloned.shopify.as_ref().unwrap().as_ref().unwrap()
    ));
    let absent = app_state(pool.clone(), settings(&json!(null))).unwrap();
    assert!(absent.shopify.unwrap().is_none());
    let invalid = app_state(
        pool,
        settings(&json!({"timeout_secs":3,"admin_api_url":"invalid-url","access_token":"fixture"})),
    )
    .unwrap();
    let cause = invalid.shopify.err().unwrap();
    assert!(matches!(
        cause.kind(),
        crate::client::ClientErrorKind::InvalidInput("invalid Shopify URL")
    ));
    assert_eq!(
        cause.location().file(),
        "services/api/src/client/shopify.rs"
    );
    let error = crate::error::AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Shopify report unavailable",
    )
    .with_source(cause);
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
}

#[tokio::test]
async fn resend_state_reuses_client_and_preserves_missing_key_and_initialization_failures() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    for (key, url, expected) in [
        (None, "http://localhost:1234", "absent"),
        (Some(""), "http://localhost:1234", "absent"),
        (Some(" "), "http://localhost:1234", "absent"),
        (Some("fixture-key"), "http://localhost:1234", "present"),
        (Some("fixture-key"), "invalid-url", "error"),
    ] {
        let mut settings = settings(&json!(null));
        settings.client.as_mut().unwrap().resend = Some(crate::config::ResendConfig {
            base_url: url.into(),
            timeout_secs: 1,
            default_sender: "shipping@enzumeii.com".into(),
            api_key: key.map(str::to_owned),
        });
        let state = app_state(pool.clone(), settings).unwrap();
        let cloned = state.clone();
        match expected {
            "absent" => assert!(state.resend.unwrap().is_none()),
            "present" => assert!(Arc::ptr_eq(
                state.resend.as_ref().unwrap().as_ref().unwrap(),
                cloned.resend.as_ref().unwrap().as_ref().unwrap()
            )),
            _ => assert!(matches!(
                state.resend.err().unwrap().kind(),
                crate::client::ClientErrorKind::InvalidInput(_)
            )),
        }
    }
    assert!(
        app_state(pool, settings(&json!(null)))
            .unwrap()
            .resend
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn resend_sdk_initialization_panics_are_retained_as_optional_errors() {
    const CHILD: &str = "MONOLITH_RESEND_INITIALIZATION_TEST";
    if std::env::var_os(CHILD).is_none() {
        for rate_limit in ["", "invalid", "0"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "state::tests::resend_sdk_initialization_panics_are_retained_as_optional_errors", "--nocapture"])
                .env(CHILD, "1")
                .env("RESEND_RATE_LIMIT", rate_limit)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "optional SDK initialization failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    let mut settings = settings(&json!(null));
    settings.client.as_mut().unwrap().resend = Some(crate::config::ResendConfig {
        base_url: "https://api.resend.com".into(),
        timeout_secs: 1,
        default_sender: "shipping@enzumeii.com".into(),
        api_key: Some("fixture-key".into()),
    });
    let state = app_state(pool, settings).unwrap();
    let error = state
        .resend
        .err()
        .expect("SDK initialization failure must remain available");
    assert!(matches!(
        error.kind(),
        crate::client::ClientErrorKind::InvalidInput("Resend initialization failed")
    ));
}
