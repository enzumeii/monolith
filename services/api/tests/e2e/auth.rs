//! POST /v1/auth/login · POST /v1/auth/refresh · GET /v1/auth/logout

use super::common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::dto::AdminRole;
use tower::ServiceExt;

#[tokio::test]
async fn login_succeeds_and_returns_token_pair() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "login-admin", "secret", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "login-admin", "password": "secret" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = common::body_json(response).await;
    assert_eq!(body["token_type"], "Bearer");
    assert_ne!(body["access_token"], "");
    assert_ne!(body["refresh_token"], "");
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "wrong-pass", "correct", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "wrong-pass", "password": "nope" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_issues_a_new_pair_and_can_only_be_used_once() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "refresh-admin", "secret", AdminRole::Admin).await;
    let router = common::router(db.pool());

    let login = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "refresh-admin", "password": "secret" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let refresh_token = common::body_json(login).await["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    let first = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": refresh_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    // The same refresh token must not be reusable (single-consumption).
    let second = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": refresh_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(second.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn refresh_rejects_unknown_token() {
    let db = common::test_database().await;
    let response = common::router(db.pool())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": "stale" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn logout_returns_no_content() {
    let db = common::test_database().await;
    let id = common::seed_admin(&db.pool(), "logout-admin", "secret", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(common::authed(
            "GET",
            "/v1/auth/logout",
            id,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn protected_routes_reject_malformed_bearer_token() {
    let db = common::test_database().await;
    let response = common::router(db.pool())
        .oneshot(
            Request::builder()
                .uri("/v1/admins")
                .header("authorization", "Bearer not-a-jwt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}
