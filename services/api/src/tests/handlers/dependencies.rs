#![allow(clippy::unwrap_used)]
use crate::{
    client::tests::Fixture,
    state::{AppState, app_state},
};
use serde_json::{Value, json};
pub(crate) fn state(f: &Fixture) -> AppState {
    let settings = serde_json::from_value(json!({
        "client":{"shopify":{"timeout_secs":3,"admin_api_url":f.url.as_str(),"access_token":"fixture"}},
        "browser":{"allowed_origins":["http://localhost:8084"],"insecure_local":true},
        "server":{"host":"localhost","port":3000},
        "database":{"url":"unused","max_connections":1,"min_connections":1},
        "checkout_recovery":{"enabled":true,"wake_secs":60,"batch_size":10,"concurrency":2,"budget_secs":60,"lease_secs":90,"retry_backoff_secs":60,"max_retry_backoff_secs":900,"max_failures":12},"scheduler":{"implementation":"vps","poll_interval_secs":1,"retention_interval_secs":3600,"invocation_budget_secs":240,"retention_reserve_secs":5,"enabled":false,"concurrency":1,"max_attempts":2,"retry_backoff_secs":1,"lease_secs":5,"max_payload_bytes":20_971_520,"max_email_bytes":20_971_520},
        "jwt":{"secret":"1111111111111111111111111111111111111111111111111111111111111111"}
    }))
    .unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    app_state(pool, settings).unwrap()
}

#[tokio::test]
async fn client_accessor_reuses_state_and_preserves_initialization_errors() {
    use crate::client::{ClientError, ClientErrorKind};
    use axum::response::IntoResponse;
    use std::{error::Error, sync::Arc};
    let f = Fixture::new(vec![]).await;
    let mut state = state(&f);
    let first = super::shopify_client(&state).unwrap();
    let second = super::shopify_client(&state).unwrap();
    assert!(std::ptr::eq(first, second));
    for failed in [false, true] {
        state.shopify = if failed {
            Err(Arc::new(ClientError::new(ClientErrorKind::InvalidInput(
                "private initialization",
            ))))
        } else {
            Ok(None)
        };
        let error = super::shopify_client(&state).err().unwrap();
        assert_eq!(error.source().is_some(), failed);
        let response = error.into_response();
        assert_eq!(response.status(), 503);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"error":"Shopify client unavailable"})
        );
    }
    assert_eq!(f.requests().len(), 0);
}

#[tokio::test]
async fn required_payment_accessors_reuse_clients_and_retain_availability_errors() {
    use crate::client::{ClientError, ClientErrorKind, paypal::PaypalClient, stripe::StripeClient};
    use std::{error::Error, sync::Arc};
    let f = Fixture::new(vec![]).await;
    let mut state = state(&f);
    for (error, message) in [
        (
            super::stripe_client(&state).err().unwrap(),
            "Stripe client unavailable",
        ),
        (
            super::paypal_client(&state).err().unwrap(),
            "PayPal client unavailable",
        ),
    ] {
        assert!(error.source().is_none());
        assert_payment_error(error, message).await;
    }
    let stripe_config = serde_json::from_value(
        json!({"base_url": f.url.as_str(), "secret_key":"fixture", "timeout_secs":3, "checkout_api_version":"2026-08-26.dahlia"}),
    )
    .unwrap();
    let paypal_config = serde_json::from_value(json!({"base_url": f.url.as_str(), "client_id":"fixture", "client_secret":"fixture", "timeout_secs":3})).unwrap();
    state.stripe = Ok(Some(Arc::new(StripeClient::new(&stripe_config).unwrap())));
    state.paypal = Ok(Some(Arc::new(PaypalClient::new(&paypal_config).unwrap())));
    assert!(std::ptr::eq(
        super::stripe_client(&state).unwrap(),
        super::stripe_client(&state).unwrap()
    ));
    assert!(std::ptr::eq(
        super::paypal_client(&state).unwrap(),
        super::paypal_client(&state).unwrap()
    ));
    state.stripe = Err(Arc::new(ClientError::new(ClientErrorKind::InvalidInput(
        "private initialization",
    ))));
    state.paypal = Err(Arc::new(ClientError::new(ClientErrorKind::InvalidInput(
        "private initialization",
    ))));
    for (error, message) in [
        (
            super::stripe_client(&state).err().unwrap(),
            "Stripe client unavailable",
        ),
        (
            super::paypal_client(&state).err().unwrap(),
            "PayPal client unavailable",
        ),
    ] {
        assert!(error.source().is_some());
        assert_payment_error(error, message).await;
    }
    assert_eq!(f.requests().len(), 0);
}

async fn assert_payment_error(error: crate::error::AppError, message: &str) {
    use axum::response::IntoResponse;
    let response = error.into_response();
    assert_eq!(response.status(), 503);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":message})
    );
}
