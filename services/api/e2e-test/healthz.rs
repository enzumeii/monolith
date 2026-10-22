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
    assert_eq!(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
        "OK"
    );
}

#[tokio::test]
async fn native_alias_matches_public_auth_and_unknown_api_never_returns_html() {
    let fixture = common::fixture().await;
    let client = common::client().await;
    for prefix in ["/v1", "/api/v1"] {
        let response = client.send(Request::builder().method("POST").uri(format!("{prefix}/auth/login")).header("x-session-mode", "cookie").header("origin", "http://127.0.0.1:8084").header("content-type", "application/json").body(Body::from(serde_json::json!({"username":fixture.username("root"),"password":"fixture-root-password"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            common::body_json(response).await["admin_id"],
            fixture.root_id()
        );
    }
    for uri in ["/api/unknown", "/api/_cron"] {
        let response = client
            .send(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_ne!(
            response
                .headers()
                .get("content-type")
                .map(|value| value.to_str().unwrap()),
            Some("text/html")
        );
    }
}
