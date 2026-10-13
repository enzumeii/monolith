//! POST /v1/auth/login · POST /v1/auth/refresh · POST /v1/auth/logout

use super::super::common;
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
    let id = common::seed_admin(
        &db.pool(),
        "logout-admin",
        "test-password",
        AdminRole::Admin,
    )
    .await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let tokens = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "logout-admin".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let response = common::router(db.pool())
        .oneshot(common::authed_json(
            "POST",
            "/v1/auth/logout",
            id,
            AdminRole::Admin,
            &serde_json::json!({"refresh_token":tokens.refresh_token}),
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

use api::services::auth;
use domain::dto::{LoginRequest, RefreshRequest};

#[tokio::test]
async fn failed_refresh_replacement_does_not_consume_original() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "rollback", "test-password", AdminRole::Admin).await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let tokens = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "rollback".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE FUNCTION reject_refresh_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected storage failure'; END $$; CREATE TRIGGER reject_refresh BEFORE INSERT ON refresh_tokens FOR EACH ROW EXECUTE FUNCTION reject_refresh_insert();").execute(&db.pool()).await.unwrap();
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token.clone()
            }
        )
        .await
        .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER reject_refresh ON refresh_tokens")
        .execute(&db.pool())
        .await
        .unwrap();
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token
            }
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn concurrent_refresh_has_exactly_one_success() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "concurrent", "test-password", AdminRole::Admin).await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let pool = db.pool();
    let tokens = auth::login(
        &pool,
        &key,
        LoginRequest {
            username: "concurrent".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let (first, second) = tokio::join!(
        auth::refresh(
            &pool,
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token.clone()
            }
        ),
        auth::refresh(
            &pool,
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token
            }
        )
    );
    assert_ne!(first.is_ok(), second.is_ok());
}

#[tokio::test]
async fn expired_refresh_is_rejected() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "expired", "test-password", AdminRole::Admin).await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let tokens = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "expired".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE refresh_tokens SET expires_at = NOW() - interval '1 second'")
        .execute(&db.pool())
        .await
        .unwrap();
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn logout_revokes_refresh_but_access_remains_valid_until_expiry() {
    let db = common::test_database().await;
    common::seed_admin(&db.pool(), "revoked", "test-password", AdminRole::Admin).await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let tokens = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "revoked".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let app = common::router(db.pool());
    let logout = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/logout")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", tokens.access_token))
                .body(Body::from(serde_json::json!({"refresh_token":tokens.refresh_token.clone()}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let protected = app
        .oneshot(
            Request::builder()
                .uri("/v1/admins")
                .header("authorization", format!("Bearer {}", tokens.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::OK);
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: tokens.refresh_token
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn password_update_preserves_access_until_expiry() {
    let db = common::test_database().await;
    let id = common::seed_admin(&db.pool(), "reset", "test-password", AdminRole::Admin).await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let tokens = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "reset".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let app = common::router(db.pool());
    let response = app
        .clone()
        .oneshot(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
            &serde_json::json!({"password":"replacement-password"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let protected = app
        .oneshot(
            Request::builder()
                .uri("/v1/admins")
                .header("authorization", format!("Bearer {}", tokens.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::OK);
}

#[tokio::test]
async fn logout_only_revokes_current_browser_family() {
    let db = common::test_database().await;
    let id = common::seed_admin(
        &db.pool(),
        "two-browsers",
        "test-password",
        AdminRole::Admin,
    )
    .await;
    let key = hex::decode(common::JWT_SECRET).unwrap();
    let first = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "two-browsers".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let second = auth::login(
        &db.pool(),
        &key,
        LoginRequest {
            username: "two-browsers".into(),
            password: "test-password".into(),
        },
    )
    .await
    .unwrap();
    let first = auth::refresh(
        &db.pool(),
        &key,
        RefreshRequest {
            refresh_token: first.refresh_token,
        },
    )
    .await
    .unwrap();
    let response = common::router(db.pool())
        .oneshot(common::authed_json(
            "POST",
            "/v1/auth/logout",
            id,
            AdminRole::Admin,
            &serde_json::json!({"refresh_token":first.refresh_token.clone()}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: first.refresh_token
            }
        )
        .await
        .is_err()
    );
    assert!(
        auth::refresh(
            &db.pool(),
            &key,
            RefreshRequest {
                refresh_token: second.refresh_token
            }
        )
        .await
        .is_ok()
    );
}
