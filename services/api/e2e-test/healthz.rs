//! GET /healthz

use super::common;
use axum::body::Body;
use axum::http::{Request, StatusCode};

#[tokio::test]
async fn returns_ok() {
    let _db = common::fixture().await;
    let response = common::client()
        .await
        .send(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
