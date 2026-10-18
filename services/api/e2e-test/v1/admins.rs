//! GET /v1/admins · GET /v1/admins/{id} · PATCH /v1/admins/{id}
//! POST /v1/admins · DELETE /v1/admins/{id}

use super::super::common;
use super::auth::{login, refresh};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::dto::AdminRole;

#[tokio::test]
async fn list_returns_seeded_super_admin_with_default_pagination() {
    let db = common::fixture().await;
    let response = common::client()
        .await
        .send(common::authed(
            "GET",
            "/v1/admins",
            db.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = common::body_json(response).await;
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 25);
    assert_ne!(body["admins"].as_array().unwrap().len(), 0);
    assert!(body["admins"].as_array().unwrap().len() <= 25);
}

#[tokio::test]
async fn list_rejects_page_zero() {
    let db = common::fixture().await;
    let response = common::client()
        .await
        .send(common::authed(
            "GET",
            "/v1/admins?page=0",
            db.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_returns_the_admin() {
    let db = common::fixture().await;
    let id = common::seed_admin(&db, db.username("found-admin"), "secret", AdminRole::Admin).await;

    let response = common::client()
        .await
        .send(common::authed(
            "GET",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(response).await["username"],
        db.username("found-admin")
    );
}

#[tokio::test]
async fn missing_admin_targets_return_not_authorized() {
    let db = common::fixture().await;
    let client = common::client().await;
    for method in ["GET", "PATCH", "DELETE"] {
        let uri = "/v1/admins/9223372036854775807";
        let request = if method == "PATCH" {
            common::authed_json(
                method,
                uri,
                db.root_id(),
                AdminRole::SuperAdmin,
                &serde_json::json!({"username":"missing_target"}),
            )
        } else {
            common::authed(method, uri, db.root_id(), AdminRole::SuperAdmin)
        };
        let response = client.send(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method}");
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert_eq!(
            common::body_json(response).await,
            serde_json::json!({"error":"Not authorized"})
        );
    }
}

#[tokio::test]
async fn update_allows_self_rename() {
    let db = common::fixture().await;
    let id = common::seed_admin(&db, db.username("self-admin"), "secret", AdminRole::Admin).await;

    let response = common::client()
        .await
        .send(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
            &serde_json::json!({ "username": db.username("renamed"), "password": null }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(response).await["username"],
        db.username("renamed")
    );
}

#[tokio::test]
async fn update_rejects_other_admins_records() {
    let db = common::fixture().await;
    let target =
        common::seed_admin(&db, db.username("target-admin"), "secret", AdminRole::Admin).await;

    let response = common::client()
        .await
        .send(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{target}"),
            target + 1000,
            AdminRole::Admin,
            &serde_json::json!({ "username": db.username("renamed"), "password": null }),
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
    let db = common::fixture().await;
    let target = common::seed_admin(
        &db,
        db.username("cross-target"),
        "old-secret",
        AdminRole::Admin,
    )
    .await;
    let super_admin = common::seed_admin(
        &db,
        db.username("cross-super"),
        "super-secret",
        AdminRole::SuperAdmin,
    )
    .await;

    // Both accounts hold a live refresh token.
    for (username, password) in [
        (db.username("cross-target"), "old-secret"),
        (db.username("cross-super"), "super-secret"),
    ] {
        let login = common::client()
            .await
            .send(
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

    let response = common::client()
        .await
        .send(common::authed_json(
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
    let login = common::client().await
        .send(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": db.username("cross-target"), "password": "new-secret-password" })
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
    let db = common::fixture().await;
    let router = common::client().await;

    let by_admin = router
        .clone()
        .send(common::authed_json(
            "POST",
            "/v1/admins",
            db.root_id(),
            AdminRole::Admin,
            &serde_json::json!({ "username": db.username("new"), "password": "long-test-password", "role": "ADMIN" }),
        ))
        .await
        .unwrap();
    assert_eq!(by_admin.status(), StatusCode::FORBIDDEN);

    let by_super = router
        .send(common::authed_json(
            "POST",
            "/v1/admins",
            db.root_id(),
            AdminRole::SuperAdmin,
            &serde_json::json!({ "username": db.username("new"), "password": "long-test-password", "role": "ADMIN" }),
        ))
        .await
        .unwrap();
    assert_eq!(by_super.status(), StatusCode::CREATED);
    assert_eq!(
        common::body_json(by_super).await["username"],
        db.username("new")
    );
}

/// A `SuperAdmin` cannot delete their own account, even though they may
/// delete other admins.
#[tokio::test]
async fn delete_rejects_self_delete() {
    let db = common::fixture().await;
    let self_admin = common::seed_admin(
        &db,
        db.username("self-delete"),
        "secret",
        AdminRole::SuperAdmin,
    )
    .await;

    let response = common::client()
        .await
        .send(common::authed(
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
    let db = common::fixture().await;
    let target =
        common::seed_admin(&db, db.username("deletable"), "secret", AdminRole::Admin).await;

    let by_admin = common::client()
        .await
        .send(common::authed(
            "DELETE",
            &format!("/v1/admins/{target}"),
            db.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(by_admin.status(), StatusCode::FORBIDDEN);

    let by_super = common::client()
        .await
        .send(common::authed(
            "DELETE",
            &format!("/v1/admins/{target}"),
            db.root_id(),
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(by_super.status(), StatusCode::OK);
    assert_eq!(
        common::body_json(by_super).await["username"],
        db.username("deletable")
    );

    // The seeded super-admin cannot be deleted by a *different* super-admin
    // (self-delete is covered separately by delete_rejects_self_delete).
    let sa_id = db.root_id();
    let other_super = common::seed_admin(
        &db,
        db.username("other-super"),
        "test-password",
        AdminRole::SuperAdmin,
    )
    .await;
    let protected = common::client()
        .await
        .send(common::authed(
            "DELETE",
            &format!("/v1/admins/{sa_id}"),
            other_super,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_rejects_empty_credentials_and_reports_duplicate_username() {
    let db = common::fixture().await;
    let app = common::client().await;
    for payload in [
        serde_json::json!({"username":"", "password":"long-test-password", "role":"ADMIN"}),
        serde_json::json!({"username":db.username("valid"), "password":"", "role":"ADMIN"}),
    ] {
        let response = app
            .clone()
            .send(common::authed_json(
                "POST",
                "/v1/admins",
                db.root_id(),
                AdminRole::SuperAdmin,
                &payload,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    common::seed_admin(
        &db,
        db.username("existing"),
        "long-test-password",
        AdminRole::Admin,
    )
    .await;
    let response = app.send(common::authed_json("POST", "/v1/admins", db.root_id(), AdminRole::SuperAdmin, &serde_json::json!({"username":db.username("existing"), "password":"long-test-password", "role":"ADMIN"}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn failed_session_revocation_rolls_back_password_update() {
    let db = common::fixture().await;
    let id = common::seed_admin(
        &db,
        db.username("rollback-update"),
        "old-password",
        AdminRole::Admin,
    )
    .await;

    login(domain::dto::LoginRequest {
        username: db.username("rollback-update"),
        password: "old-password".into(),
    })
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {}_reject_revocation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected revocation failure'; END $$; CREATE TRIGGER {}_reject_revoke BEFORE DELETE ON refresh_tokens FOR EACH ROW WHEN (OLD.admin_id = {}) EXECUTE FUNCTION {}_reject_revocation();",db.prefix(),db.prefix(),id,db.prefix()))).execute(&db.pool()).await.unwrap();
    let response = common::client()
        .await
        .send(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{id}"),
            id,
            AdminRole::Admin,
            &serde_json::json!({"password":"new-password-value"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        login(domain::dto::LoginRequest {
            username: db.username("rollback-update"),
            password: "old-password".into()
        })
        .await
        .is_ok()
    );
    assert!(
        login(domain::dto::LoginRequest {
            username: db.username("rollback-update"),
            password: "new-password-value".into()
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn completed_refresh_is_revoked_by_a_following_password_update() {
    let db = common::fixture().await;
    let pool = db.pool();
    let id = common::seed_admin(
        &db,
        db.username("race-update"),
        "old-password",
        AdminRole::Admin,
    )
    .await;

    let tokens = login(domain::dto::LoginRequest {
        username: db.username("race-update"),
        password: "old-password".into(),
    })
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {}_pause_refresh() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(48721013); RETURN NEW; END $$; CREATE TRIGGER {}_pause_refresh BEFORE INSERT ON refresh_tokens FOR EACH ROW WHEN (NEW.admin_id = {}) EXECUTE FUNCTION {}_pause_refresh();",db.prefix(),db.prefix(),id,db.prefix()))).execute(&pool).await.unwrap();
    let mut control = pool.acquire().await.unwrap().detach();
    sqlx::query("SELECT pg_advisory_lock(48721013)")
        .execute(&mut control)
        .await
        .unwrap();

    let rotation = tokio::spawn(async move {
        refresh(domain::dto::RefreshRequest {
            refresh_token: tokens.refresh_token,
        })
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5),async{
        loop{
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype = 'advisory' AND objid = 48721013 AND NOT granted)").fetch_one(&pool).await.unwrap();
            if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();

    let update = tokio::spawn(async move {
        update_admin(
            id,
            AdminRole::Admin,
            id,
            domain::dto::UpdateAdminRequest {
                username: None,
                password: Some("new-password-value".into()),
            },
        )
        .await
    });
    sqlx::query("SELECT pg_advisory_unlock(48721013)")
        .execute(&mut control)
        .await
        .unwrap();
    let replacement = rotation.await.unwrap().unwrap();
    update.await.unwrap().unwrap();
    assert!(
        refresh(domain::dto::RefreshRequest {
            refresh_token: replacement.refresh_token
        })
        .await
        .is_err()
    );
}

async fn update_admin(
    actor: i64,
    role: AdminRole,
    id: i64,
    request: domain::dto::UpdateAdminRequest,
) -> Result<domain::dto::AdminDTO, StatusCode> {
    let response = common::client()
        .await
        .send(common::authed_json(
            "PATCH",
            &format!("/v1/admins/{id}"),
            actor,
            role,
            &serde_json::to_value(request).unwrap(),
        ))
        .await
        .unwrap();
    if !response.status().is_success() {
        return Err(response.status());
    }
    serde_json::from_value(common::body_json(response).await).map_err(|_| StatusCode::BAD_GATEWAY)
}

#[tokio::test]
async fn list_covers_first_last_and_empty_pages_without_resetting_shared_rows() {
    let db = common::fixture().await;
    for label in ["page-a", "page-b", "page-c"] {
        common::seed_admin(
            &db,
            db.username(label),
            "fixture-password",
            AdminRole::Admin,
        )
        .await;
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin")
        .fetch_one(&db.pool())
        .await
        .unwrap();
    let last = (count + 1) / 2;
    for (page, has_next, expected_len) in [
        (1, true, 2),
        (last, false, count - 2 * (last - 1)),
        (last + 1, false, 0),
    ] {
        let response = common::client()
            .await
            .send(common::authed(
                "GET",
                &format!("/v1/admins?page={page}&per_page=2"),
                db.root_id(),
                AdminRole::SuperAdmin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = common::body_json(response).await;
        assert_eq!(body["page"], page);
        assert_eq!(body["has_next"], has_next);
        assert_eq!(
            i64::try_from(body["admins"].as_array().unwrap().len()).unwrap(),
            expected_len
        );
    }
}
#[tokio::test]
async fn cookie_self_update_revokes_account_sessions() {
    let db = common::fixture().await;
    let a = super::auth::cookie_pair(
        &super::auth::cookie_login(&db.username("root"), "fixture-root-password").await,
    );
    let b = super::auth::cookie_pair(
        &super::auth::cookie_login(&db.username("root"), "fixture-root-password").await,
    );
    let uri = format!("/v1/admins/{}", db.root_id());
    let response = common::client()
        .await
        .send(
            Request::builder()
                .method("PATCH")
                .uri(&uri)
                .header("x-session-mode", "cookie")
                .header("origin", super::auth::BROWSER_ORIGIN)
                .header("cookie", &a)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"username": db.username("renamed")}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        common::body_json(response).await["username"],
        db.username("renamed")
    );
    for cookie in [&a, &b] {
        assert_eq!(
            super::auth::cookie_request("POST", "/v1/auth/refresh", cookie)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            super::auth::cookie_request("GET", &uri, cookie)
                .await
                .status(),
            StatusCode::OK,
            "access JWT remains valid until expiry"
        );
    }
}
