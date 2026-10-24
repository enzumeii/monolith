#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::StatusCode;
use serde_json::json;
fn settings(shopify: &serde_json::Value) -> Settings {
    serde_json::from_value(json!({"client":{"shopify":shopify},"browser":{"allowed_origins":["http://localhost:8084"],"insecure_local":true},"server":{"host":"localhost","port":3000},"database":{"url":"unused","max_connections":1,"min_connections":1},"checkout_recovery":{"enabled":true,"wake_secs":60,"batch_size":10,"concurrency":2,"budget_secs":60,"lease_secs":90,"retry_backoff_secs":60,"max_retry_backoff_secs":900,"max_failures":12},"scheduler":{"implementation":"vps","poll_interval_secs":1,"retention_interval_secs":3600,"invocation_budget_secs":240,"retention_reserve_secs":5,"enabled":false,"concurrency":1,"max_attempts":2,"retry_backoff_secs":1,"lease_secs":5,"max_payload_bytes":20_971_520,"max_email_bytes":20_971_520},"jwt":{"secret":"1111111111111111111111111111111111111111111111111111111111111111"}})).unwrap()
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
        "Shopify client unavailable",
    )
    .with_source(cause);
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
}

#[tokio::test]
async fn resend_state_reuses_client_and_retains_missing_or_empty_key_errors() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    for key in [None, Some(""), Some(" "), Some("fixture-key")] {
        let mut settings = settings(&json!(null));
        settings.client.as_mut().unwrap().resend = Some(crate::config::ResendConfig {
            timeout_secs: Some(10),
            api_key: key.map(str::to_owned),
        });
        let state = app_state(pool.clone(), settings).unwrap();
        let cloned = state.clone();
        if key.is_some_and(|key| !key.trim().is_empty()) {
            assert!(Arc::ptr_eq(
                state.resend.as_ref().unwrap().as_ref().unwrap(),
                cloned.resend.as_ref().unwrap().as_ref().unwrap()
            ));
        } else {
            let error = state
                .resend
                .err()
                .expect("constructor error must be retained");
            assert!(matches!(
                error.kind(),
                crate::client::ClientErrorKind::InvalidInput("Resend API key is missing")
            ));
            assert!(error.location().file().ends_with("client/resend.rs"));
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
        for (variable, value) in [
            ("RESEND_RATE_LIMIT", ""),
            ("RESEND_RATE_LIMIT", "invalid"),
            ("RESEND_RATE_LIMIT", "0"),
            ("RESEND_BASE_URL", "invalid-url"),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "state::tests::resend_sdk_initialization_panics_are_retained_as_optional_errors", "--nocapture"])
                .env(CHILD, "1")
                .env_remove("RESEND_RATE_LIMIT")
                .env_remove("RESEND_BASE_URL")
                .env(variable, value)
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
        timeout_secs: Some(10),
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

#[tokio::test]
async fn paypal_state_is_optional_shared_and_retains_invalid_configuration() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    let absent = app_state(pool.clone(), settings(&json!(null))).unwrap();
    assert!(absent.paypal.unwrap().is_none());
    for (id, secret, valid) in [
        (None, None, false),
        (Some(""), Some(""), true),
        (Some(" "), Some(" "), true),
        (Some("id"), None, false),
        (None, Some("secret"), false),
        (Some("id"), Some("secret"), true),
    ] {
        let mut settings = settings(&json!(null));
        settings.client.as_mut().unwrap().paypal = Some(crate::config::PaypalConfig {
            base_url: "https://api-m.sandbox.paypal.com/".into(),
            timeout_secs: 10,
            client_id: id.map(str::to_owned),
            client_secret: secret.map(str::to_owned),
        });
        let state = app_state(pool.clone(), settings).unwrap();
        if valid {
            assert!(Arc::ptr_eq(
                state.paypal.as_ref().unwrap().as_ref().unwrap(),
                state.clone().paypal.as_ref().unwrap().as_ref().unwrap()
            ));
        } else {
            let error = state
                .paypal
                .err()
                .expect("missing credentials must retain constructor error");
            assert!(matches!(
                error.kind(),
                crate::client::ClientErrorKind::InvalidInput(_)
            ));
            assert!(error.location().file().ends_with("client/paypal.rs"));
        }
    }
    let mut settings = settings(&json!(null));
    settings.client.as_mut().unwrap().paypal = Some(crate::config::PaypalConfig {
        base_url: "invalid".into(),
        timeout_secs: 10,
        client_id: Some("id".into()),
        client_secret: Some("secret".into()),
    });
    assert!(app_state(pool, settings).unwrap().paypal.is_err());
}

#[tokio::test]
async fn stripe_state_shares_client_and_preserves_optional_errors() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    let absent = app_state(pool.clone(), settings(&json!(null))).unwrap();
    assert!(absent.stripe.unwrap().is_none());
    for key in [None, Some(""), Some("fixture")] {
        let mut s = settings(&json!(null));
        s.client.as_mut().unwrap().stripe = Some(crate::config::StripeConfig {
            checkout_api_version: "2026-08-26.dahlia".into(),
            base_url: "https://api.stripe.com/v1/".into(),
            timeout_secs: 3,
            secret_key: key.map(str::to_owned),
        });
        let state = app_state(pool.clone(), s).unwrap();
        if key == Some("fixture") {
            let cloned = state.clone();
            assert!(Arc::ptr_eq(
                state.stripe.as_ref().unwrap().as_ref().unwrap(),
                cloned.stripe.as_ref().unwrap().as_ref().unwrap()
            ));
        } else {
            assert!(state.stripe.is_err());
        }
    }
}

#[tokio::test]
async fn s3_state_is_optional_shared_and_preserves_initialization_errors() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    assert!(
        app_state(pool.clone(), settings(&json!(null)))
            .unwrap()
            .s3
            .unwrap()
            .is_none()
    );
    for key in [None, Some(""), Some("synthetic-access")] {
        let mut settings = settings(&json!(null));
        settings.client.as_mut().unwrap().s3 = Some(crate::config::S3Config {
            endpoint: "https://example.test/storage/v1/s3".into(),
            region: "ap-southeast-1".into(),
            bucket: "reports".into(),
            force_path_style: true,
            timeout_secs: 3,
            signed_url_ttl_secs: 300,
            access_key_id: key.map(str::to_owned),
            secret_access_key: Some("synthetic-secret".into()),
        });
        let state = app_state(pool.clone(), settings).unwrap();
        if key == Some("synthetic-access") {
            assert!(Arc::ptr_eq(
                state.s3.as_ref().unwrap().as_ref().unwrap(),
                state.clone().s3.as_ref().unwrap().as_ref().unwrap()
            ));
        } else {
            let error = required_client(&state.s3, "Report storage unavailable")
                .err()
                .unwrap();
            assert_eq!(
                axum::response::IntoResponse::into_response(error).status(),
                503
            );
        }
    }
}
