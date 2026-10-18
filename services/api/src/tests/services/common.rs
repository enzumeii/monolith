#![allow(clippy::unwrap_used)]
use crate::{
    client::tests::Fixture,
    error::AppError,
    models::shopify::ShopifyId,
    state::{AppState, app_state},
};
use serde_json::{Value, json};

pub(crate) fn id(kind: &str, n: u64) -> ShopifyId {
    ShopifyId::new(format!("gid://shopify/{kind}/{n}")).unwrap()
}

pub(crate) fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
    let mut value = json!({"pageInfo":{"hasNextPage":next.is_some(),"endCursor":next}});
    value["nodes"] = Value::Array(nodes);
    value
}

pub(crate) fn state(f: &Fixture) -> AppState {
    let settings = serde_json::from_value(json!({
        "client":{"shopify":{"timeout_secs":3,"admin_api_url":f.url.as_str(),"access_token":"fixture"}},
        "browser":{"public_origin":"http://localhost:8084","insecure_local":true},
        "server":{"host":"localhost","port":3000},
        "database":{"url":"unused","max_connections":1,"min_connections":1},
        "jwt":{"secret":"1111111111111111111111111111111111111111111111111111111111111111"}
    }))
    .unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    app_state(pool, settings).unwrap()
}

pub(crate) async fn assert_failure(f: &Fixture, error: AppError, count: usize) {
    let response = axum::response::IntoResponse::into_response(error);
    assert_eq!(response.status(), 502);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(body.get("products").is_none());
    assert!(!body.to_string().contains("private"));
    assert_eq!(f.requests().len(), count);
}

#[tokio::test]
async fn client_accessor_reuses_state_and_preserves_caller_errors() {
    use crate::client::{ClientError, ClientErrorKind};
    use axum::response::IntoResponse;
    use std::{error::Error, sync::Arc};
    let f = Fixture::new(vec![]).await;
    let mut state = state(&f);
    let first = super::shopify_client(&state, "Shopify report unavailable").unwrap();
    let second = super::shopify_client(&state, "Shopify inventory activation unavailable").unwrap();
    assert!(std::ptr::eq(first, second));
    for failed in [false, true] {
        state.shopify = if failed {
            Err(Arc::new(ClientError::new(ClientErrorKind::InvalidInput(
                "private initialization",
            ))))
        } else {
            Ok(None)
        };
        for message in [
            "Shopify report unavailable",
            "Shopify inventory activation unavailable",
        ] {
            let error = super::shopify_client(&state, message).err().unwrap();
            assert_eq!(error.source().is_some(), failed);
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
    }
    assert_eq!(f.requests().len(), 0);
}
