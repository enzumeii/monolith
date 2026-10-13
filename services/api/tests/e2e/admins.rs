//! GET /v1/admins · GET /v1/admins/{id} · PATCH /v1/admins/{id}
//! POST /v1/admins · DELETE /v1/admins/{id}

use super::common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::dto::AdminRole;
use tower::ServiceExt;

#[tokio::test]
async fn list_returns_seeded_super_admin_with_default_pagination() {
    let db = common::test_database().await;
    let response = common::router(db.pool())
        .oneshot(common::authed("GET", "/v1/admins", 1, AdminRole::Admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = common::body_json(response).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 25);
    assert_eq!(body["has_next"], false);
    assert_eq!(body["admins"].as_array().unwrap().len(), 1);
    assert_eq!(body["admins"][0]["role"], "SUPER_ADMIN");
}

#[tokio::test]
async fn list_rejects_page_zero() {
    let db = common::test_database().await;
    let response = common::router(db.pool())
        .oneshot(common::authed(
            "GET",
            "/v1/admins?page=0",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_returns_the_admin() {
    let db = common::test_database().await;
    let id = common::seed_admin(&db.pool(), "found-admin", "secret", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(common::authed(
            "GET",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::body_json(response).await["username"], "found-admin");
}

#[tokio::test]
async fn get_returns_not_found_for_missing() {
    let db = common::test_database().await;
    let response = common::router(db.pool())
        .oneshot(common::authed("GET", "/v1/admins/999", 1, AdminRole::Admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_allows_self_rename() {
    let db = common::test_database().await;
    let id = common::seed_admin(&db.pool(), "self-admin", "secret", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
            &serde_json::json!({ "username": "renamed", "password": null }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::body_json(response).await["username"], "renamed");
}

#[tokio::test]
async fn update_rejects_other_admins_records() {
    let db = common::test_database().await;
    let target = common::seed_admin(&db.pool(), "target-admin", "secret", AdminRole::Admin).await;

    let response = common::router(db.pool())
        .oneshot(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{target}"),
            target + 1000,
            AdminRole::Admin,
            &serde_json::json!({ "username": "renamed", "password": null }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// A `SuperAdmin` may update another account. The update revokes the target's
/// refresh tokens (so an old session cannot survive a password change) while
/// the acting `SuperAdmin` keeps its own.
#[tokio::test]
async fn update_allows_super_admin_cross_account_and_revokes_target_tokens() {
    let db = common::test_database().await;
    let target =
        common::seed_admin(&db.pool(), "cross-target", "old-secret", AdminRole::Admin).await;
    let super_admin = common::seed_admin(
        &db.pool(),
        "cross-super",
        "super-secret",
        AdminRole::SuperAdmin,
    )
    .await;

    // Both accounts hold a live refresh token.
    for (username, password) in [
        ("cross-target", "old-secret"),
        ("cross-super", "super-secret"),
    ] {
        let login = common::router(db.pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "username": username, "password": password })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
    }

    let response = common::router(db.pool())
        .oneshot(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{target}"),
            super_admin,
            AdminRole::SuperAdmin,
            &serde_json::json!({ "password": "new-secret-password" }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let target_tokens: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE admin_id = $1")
            .bind(target)
            .fetch_one(&db.pool())
            .await
            .unwrap();
    let super_tokens: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE admin_id = $1")
            .bind(super_admin)
            .fetch_one(&db.pool())
            .await
            .unwrap();
    assert_eq!(target_tokens, 0);
    assert_eq!(super_tokens, 1);

    // The target can authenticate with the rotated password.
    let login = common::router(db.pool())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "cross-target", "password": "new-secret-password" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn add_requires_super_admin() {
    let db = common::test_database().await;
    let router = common::router(db.pool());

    let by_admin = router
        .clone()
        .oneshot(common::authed_json(
            "POST",
            "/v1/admins",
            1,
            AdminRole::Admin,
            &serde_json::json!({ "username": "new", "password": "long-test-password", "role": "ADMIN" }),
        ))
        .await
        .unwrap();
    assert_eq!(by_admin.status(), StatusCode::FORBIDDEN);

    let by_super = router
        .oneshot(common::authed_json(
            "POST",
            "/v1/admins",
            1,
            AdminRole::SuperAdmin,
            &serde_json::json!({ "username": "new", "password": "long-test-password", "role": "ADMIN" }),
        ))
        .await
        .unwrap();
    assert_eq!(by_super.status(), StatusCode::CREATED);
    assert_eq!(common::body_json(by_super).await["username"], "new");
}

/// A `SuperAdmin` cannot delete their own account, even though they may
/// delete other admins.
#[tokio::test]
async fn delete_rejects_self_delete() {
    let db = common::test_database().await;
    let self_admin =
        common::seed_admin(&db.pool(), "self-delete", "secret", AdminRole::SuperAdmin).await;

    let response = common::router(db.pool())
        .oneshot(common::authed(
            "DELETE",
            &format!("/v1/admins/{self_admin}"),
            self_admin,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM admin WHERE id = $1)")
        .bind(self_admin)
        .fetch_one(&db.pool())
        .await
        .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn delete_requires_super_admin_and_protects_super_admins() {
    let db = common::test_database().await;
    let target = common::seed_admin(&db.pool(), "deletable", "secret", AdminRole::Admin).await;

    let by_admin = common::router(db.pool())
        .oneshot(common::authed(
            "DELETE",
            &format!("/v1/admins/{target}"),
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(by_admin.status(), StatusCode::FORBIDDEN);

    let by_super = common::router(db.pool())
        .oneshot(common::authed(
            "DELETE",
            &format!("/v1/admins/{target}"),
            1,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(by_super.status(), StatusCode::OK);
    assert_eq!(common::body_json(by_super).await["username"], "deletable");

    // The seeded super-admin cannot be deleted by a *different* super-admin
    // (self-delete is covered separately by delete_rejects_self_delete).
    let sa_id: i64 =
        sqlx::query_scalar("SELECT id FROM admin WHERE role = 'SUPER_ADMIN'::ADMIN_ROLE LIMIT 1")
            .fetch_one(&db.pool())
            .await
            .unwrap();
    let protected = common::router(db.pool())
        .oneshot(common::authed(
            "DELETE",
            &format!("/v1/admins/{sa_id}"),
            sa_id + 1,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_rejects_empty_credentials_and_reports_duplicate_username() {
    let db = common::test_database().await;
    let app = common::router(db.pool());
    for payload in [
        serde_json::json!({"username":"", "password":"long-test-password", "role":"ADMIN"}),
        serde_json::json!({"username":"valid", "password":"", "role":"ADMIN"}),
    ] {
        let response = app
            .clone()
            .oneshot(common::authed_json(
                "POST",
                "/v1/admins",
                1,
                AdminRole::SuperAdmin,
                &payload,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    common::seed_admin(
        &db.pool(),
        "existing",
        "long-test-password",
        AdminRole::Admin,
    )
    .await;
    let response = app.oneshot(common::authed_json("POST", "/v1/admins", 1, AdminRole::SuperAdmin, &serde_json::json!({"username":"existing", "password":"long-test-password", "role":"ADMIN"}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
