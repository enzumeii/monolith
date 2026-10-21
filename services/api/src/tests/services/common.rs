#![allow(clippy::unwrap_used)]
use crate::{
    client::{shopify::ShopifyClient, tests::Fixture},
    error::AppError,
    models::shopify::ShopifyId,
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

pub(crate) fn client(f: &Fixture) -> ShopifyClient {
    ShopifyClient::new(&crate::config::ShopifyConfig {
        admin_api_url: f.url.to_string(),
        timeout_secs: 3,
        access_token: Some("fixture".into()),
    })
    .unwrap()
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
