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
    assert!(
        std::error::Error::source(cause.as_ref())
            .expect("constructor source")
            .to_string()
            .contains("invalid Shopify URL")
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
