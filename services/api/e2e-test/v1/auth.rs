//! POST /v1/auth/login · POST /v1/auth/refresh · POST /v1/auth/logout

use super::super::common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::dto::AdminRole;

const CUSTOM_ORIGIN: &str = "https://convention.enzumeii.com";
const VERCEL_ORIGIN: &str = "https://project.vercel.app";

pub(super) const BROWSER_ORIGIN: &str = "http://127.0.0.1:8084";

pub(super) async fn cookie_login(username: &str, password: &str) -> axum::response::Response {
    cookie_login_from_origin(username, password, BROWSER_ORIGIN).await
}

pub(super) async fn cookie_request(
    method: &str,
    uri: &str,
    cookie: &str,
) -> axum::response::Response {
    cookie_request_from_origin(method, uri, cookie, BROWSER_ORIGIN).await
}

pub(super) fn cookie_pair(response: &axum::response::Response) -> String {
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn cookie_login_from_origin(
    username: &str,
    password: &str,
    origin: &str,
) -> axum::response::Response {
    common::client()
        .await
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("x-session-mode", "cookie")
                .header("origin", origin)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"username":username,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn cookie_request_from_origin(
    method: &str,
    uri: &str,
    cookie: &str,
    origin: &str,
) -> axum::response::Response {
    common::client()
        .await
        .send(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("x-session-mode", "cookie")
                .header("origin", origin)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn login_succeeds_and_returns_token_pair() {
    let db = common::fixture().await;
    common::seed_admin(&db, db.username("login-admin"), "secret", AdminRole::Admin).await;

    let response = common::client().await
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": db.username("login-admin"), "password": "secret" })
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
    let db = common::fixture().await;
    common::seed_admin(&db, db.username("wrong-pass"), "correct", AdminRole::Admin).await;

    let response = common::client().await
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": db.username("wrong-pass"), "password": "nope" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_issues_a_new_pair_and_can_only_be_used_once() {
    let db = common::fixture().await;
    common::seed_admin(
        &db,
        db.username("refresh-admin"),
        "secret",
        AdminRole::Admin,
    )
    .await;
    let router = common::client().await;

    let login = router
        .clone()
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": db.username("refresh-admin"), "password": "secret" })
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
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
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
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
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
    let _db = common::fixture().await;
    let response = common::client()
        .await
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
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
async fn protected_routes_reject_malformed_bearer_token() {
    let _db = common::fixture().await;
    let response = common::client()
        .await
        .send(
            Request::builder()
                .uri("/api/v1/admins")
                .header("authorization", "Bearer not-a-jwt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};

#[tokio::test]
async fn failed_refresh_replacement_does_not_consume_original() {
    let db = common::fixture().await;
    common::seed_admin(
        &db,
        db.username("rollback"),
        "test-password",
        AdminRole::Admin,
    )
    .await;

    let tokens = login(LoginRequest {
        username: db.username("rollback"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {}_reject_refresh_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected storage failure'; END $$; CREATE TRIGGER {}_reject_refresh BEFORE INSERT ON refresh_tokens FOR EACH ROW WHEN (NEW.admin_id = {}) EXECUTE FUNCTION {}_reject_refresh_insert();",db.prefix(),db.prefix(),tokens.id,db.prefix()))).execute(&db.pool()).await.unwrap();
    assert!(
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token.clone()
        })
        .await
        .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {}_reject_refresh ON refresh_tokens",
        db.prefix()
    )))
    .execute(&db.pool())
    .await
    .unwrap();
    assert!(
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token
        })
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn concurrent_refresh_has_exactly_one_success() {
    let db = common::fixture().await;
    common::seed_admin(
        &db,
        db.username("concurrent"),
        "test-password",
        AdminRole::Admin,
    )
    .await;

    let tokens = login(LoginRequest {
        username: db.username("concurrent"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    let (first, second) = tokio::join!(
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token.clone()
        }),
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token
        })
    );
    assert_ne!(first.is_ok(), second.is_ok());
}

#[tokio::test]
async fn expired_refresh_is_rejected() {
    let db = common::fixture().await;
    common::seed_admin(
        &db,
        db.username("expired"),
        "test-password",
        AdminRole::Admin,
    )
    .await;

    let tokens = login(LoginRequest {
        username: db.username("expired"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    sqlx::query(
        "UPDATE refresh_tokens SET expires_at = NOW() - interval '1 second' WHERE admin_id = $1",
    )
    .bind(tokens.id)
    .execute(&db.pool())
    .await
    .unwrap();
    assert!(
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn logout_revokes_refresh_but_access_remains_valid_until_expiry() {
    let db = common::fixture().await;
    common::seed_admin(
        &db,
        db.username("revoked"),
        "test-password",
        AdminRole::Admin,
    )
    .await;

    let tokens = login(LoginRequest {
        username: db.username("revoked"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    let app = common::client().await;
    let logout = app
        .clone()
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", tokens.access_token))
                .body(Body::from(
                    serde_json::json!({"refresh_token":tokens.refresh_token.clone()}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let protected = app
        .send(
            Request::builder()
                .uri("/api/v1/admins")
                .header("authorization", format!("Bearer {}", tokens.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::OK);
    assert!(
        refresh(RefreshRequest {
            refresh_token: tokens.refresh_token
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn password_update_preserves_access_until_expiry() {
    let db = common::fixture().await;
    let id = common::seed_admin(&db, db.username("reset"), "test-password", AdminRole::Admin).await;

    let tokens = login(LoginRequest {
        username: db.username("reset"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    let app = common::client().await;
    let response = app
        .clone()
        .send(common::authed_json(
            "PATCH",
            &format!("/api/v1/admins/{id}"),
            id,
            AdminRole::Admin,
            &serde_json::json!({"password":"replacement-password"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let protected = app
        .send(
            Request::builder()
                .uri("/api/v1/admins")
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
    let db = common::fixture().await;
    let id = common::seed_admin(
        &db,
        db.username("two-browsers"),
        "test-password",
        AdminRole::Admin,
    )
    .await;

    let first = login(LoginRequest {
        username: db.username("two-browsers"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    let second = login(LoginRequest {
        username: db.username("two-browsers"),
        password: "test-password".into(),
    })
    .await
    .unwrap();
    let first = refresh(RefreshRequest {
        refresh_token: first.refresh_token,
    })
    .await
    .unwrap();
    let response = common::client()
        .await
        .send(common::authed_json(
            "POST",
            "/api/v1/auth/logout",
            id,
            AdminRole::Admin,
            &serde_json::json!({"refresh_token":first.refresh_token.clone()}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        refresh(RefreshRequest {
            refresh_token: first.refresh_token
        })
        .await
        .is_err()
    );
    assert!(
        refresh(RefreshRequest {
            refresh_token: second.refresh_token
        })
        .await
        .is_ok()
    );
}

pub(super) async fn login(request: LoginRequest) -> Result<TokenResponse, StatusCode> {
    token_request(
        "/api/v1/auth/login",
        &serde_json::to_value(request).unwrap(),
    )
    .await
}
pub(super) async fn refresh(request: RefreshRequest) -> Result<TokenResponse, StatusCode> {
    token_request(
        "/api/v1/auth/refresh",
        &serde_json::to_value(request).unwrap(),
    )
    .await
}
async fn token_request(path: &str, body: &serde_json::Value) -> Result<TokenResponse, StatusCode> {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = common::client()
        .await
        .send(request)
        .await
        .expect("deployed endpoint");
    if !response.status().is_success() {
        return Err(response.status());
    }
    serde_json::from_value(common::body_json(response).await).map_err(|_| StatusCode::BAD_GATEWAY)
}

#[tokio::test]
async fn protected_routes_reject_missing_wrong_signature_and_expired_access() {
    let db = common::fixture().await;
    let claims = serde_json::json!({"sub":db.root_id(),"username":"fixture","role":"SUPER_ADMIN","exp":4_102_444_800_u64});
    let wrong = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&[0; 32]),
    )
    .unwrap();
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &serde_json::json!({"exp":1,"sub":db.root_id(),"username":"fixture","role":"SUPER_ADMIN"}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    for token in [None, Some(wrong), Some(expired)] {
        let mut request = Request::builder().uri("/api/v1/admins");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = common::client()
            .await
            .send(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
}
#[tokio::test]
async fn login_rejects_malformed_json() {
    let _db = common::fixture().await;
    let response = common::client()
        .await
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_checked_before_password_change_can_issue_after_revocation() {
    let db = common::fixture().await;
    let pool = db.pool();
    let username = db.username("login-race");
    let id = common::seed_admin(&db, &username, "old-password", AdminRole::Admin).await;
    let changed_hash = common::hash_password("new-password-value");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {}_pause_refresh() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(48721013); RETURN NEW; END $$; CREATE TRIGGER {}_pause_refresh BEFORE INSERT ON refresh_tokens FOR EACH ROW WHEN (NEW.admin_id = {}) EXECUTE FUNCTION {}_pause_refresh();",db.prefix(),db.prefix(),id,db.prefix()))).execute(&pool).await.unwrap();
    let mut control = pool.acquire().await.unwrap().detach();
    sqlx::query("SELECT pg_advisory_lock(48721013)")
        .execute(&mut control)
        .await
        .unwrap();
    let login_name = username.clone();
    let issuance = tokio::spawn(async move {
        login(LoginRequest {
            username: login_name,
            password: "old-password".into(),
        })
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND objid=48721013 AND NOT granted)").fetch_one(&pool).await.unwrap();
        if waiting{break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }}).await.unwrap();
    // A separate connection represents another instance's committed account change.
    let mut change = pool.begin().await.unwrap();
    sqlx::query("UPDATE admin SET password_hash=$1 WHERE id=$2")
        .bind(changed_hash)
        .bind(id)
        .execute(&mut *change)
        .await
        .unwrap();
    sqlx::query("DELETE FROM refresh_tokens WHERE admin_id=$1")
        .bind(id)
        .execute(&mut *change)
        .await
        .unwrap();
    change.commit().await.unwrap();
    sqlx::query("SELECT pg_advisory_unlock(48721013)")
        .execute(&mut control)
        .await
        .unwrap();
    let issued = issuance.await.unwrap().unwrap();
    assert!(
        login(LoginRequest {
            username,
            password: "old-password".into()
        })
        .await
        .is_err()
    );
    assert!(
        refresh(RefreshRequest {
            refresh_token: issued.refresh_token
        })
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn cookie_login_hides_tokens_and_sets_cookie() {
    let db = common::fixture().await;
    let response = cookie_login(&db.username("root"), "fixture-root-password").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let cookie = axum_extra::extract::cookie::Cookie::parse(
        response.headers()["set-cookie"].to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(cookie.name(), "backoffice_auth");
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(
        cookie.same_site(),
        Some(axum_extra::extract::cookie::SameSite::Lax)
    );
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.domain(), None);
    let body = common::body_json(response).await;
    assert_eq!(body["admin_id"], db.root_id());
    assert_eq!(body["username"], db.username("root"));
    assert_eq!(body["role"], "SUPER_ADMIN");
    assert!(
        body["expires_at"].as_u64().unwrap()
            > std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
    );
    assert!(body.get("access_token").is_none());
    assert!(body.get("refresh_token").is_none());
}

#[tokio::test]
async fn cookie_refresh_rotates_once() {
    let db = common::fixture().await;
    let old = cookie_pair(&cookie_login(&db.username("root"), "fixture-root-password").await);
    let rotated = cookie_request("POST", "/api/v1/auth/refresh", &old).await;
    assert_eq!(rotated.status(), StatusCode::OK);
    let winning = cookie_pair(&rotated);
    assert_ne!(old, winning);
    let replay = cookie_request("POST", "/api/v1/auth/refresh", &old).await;
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert!(!replay.headers().contains_key("set-cookie"));
    assert_eq!(
        cookie_request("POST", "/api/v1/auth/refresh", &winning)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn cookie_logout_revokes_only_its_family() {
    let db = common::fixture().await;
    let a = cookie_pair(&cookie_login(&db.username("root"), "fixture-root-password").await);
    let b = cookie_pair(&cookie_login(&db.username("root"), "fixture-root-password").await);
    let logout = cookie_request("POST", "/api/v1/auth/logout", &a).await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(
        logout.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        cookie_request("POST", "/api/v1/auth/refresh", &a)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        cookie_request("POST", "/api/v1/auth/refresh", &b)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn cookie_mutations_require_origin() {
    let db = common::fixture().await;
    let valid = cookie_pair(&cookie_login(&db.username("root"), "fixture-root-password").await);
    for origin in [
        None,
        Some("null"),
        Some("https://foreign.example.test"),
        Some("https://convention.enzumeii.com.evil.test"),
        Some("https://evilconvention.enzumeii.com"),
        Some("http://convention.enzumeii.com"),
        Some("https://convention.enzumeii.com/"),
    ] {
        for uri in [
            "/api/v1/auth/login",
            "/api/v1/auth/refresh",
            "/api/v1/auth/logout",
        ] {
            let mut builder = Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-session-mode", "cookie")
                .header("cookie", &valid)
                .header("content-type", "application/json");
            if let Some(origin) = origin {
                builder = builder.header("origin", origin);
            }
            let body = if uri.ends_with("login") {
                Body::from(serde_json::json!({"username":db.username("root"),"password":"fixture-root-password"}).to_string())
            } else {
                Body::empty()
            };
            let response = common::client()
                .await
                .send(builder.body(body).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "{uri}: {origin:?}"
            );
            assert!(!response.headers().contains_key("set-cookie"));
            assert!(
                !response
                    .headers()
                    .contains_key("access-control-allow-origin")
            );
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
    }
    assert_eq!(
        cookie_request("POST", "/api/v1/auth/refresh", &valid)
            .await
            .status(),
        StatusCode::OK
    );
    let families: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT family_id) FROM refresh_tokens WHERE admin_id=$1",
    )
    .bind(db.root_id())
    .fetch_one(&db.pool())
    .await
    .unwrap();
    assert_eq!(families, 1, "rejected login must not issue another family");
}

#[tokio::test]
async fn explicit_invalid_bearer_never_falls_back() {
    let db = common::fixture().await;
    let cookie = cookie_pair(&cookie_login(&db.username("root"), "fixture-root-password").await);
    let uri = format!("/api/v1/admins/{}", db.root_id());
    assert_eq!(
        cookie_request("GET", &uri, &cookie).await.status(),
        StatusCode::OK
    );
    for bearer in [
        "Bearer invalid".to_string(),
        format!(
            "Bearer {}",
            common::jwt_for(db.root_id(), AdminRole::SuperAdmin)
        ),
    ] {
        let response = common::client()
            .await
            .send(
                Request::builder()
                    .uri(&uri)
                    .header("cookie", &cookie)
                    .header("authorization", &bearer)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if bearer == "Bearer invalid" {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::OK
            }
        );
    }
}

#[tokio::test]
async fn cors_preflight_matches_configured_origin() {
    let _db = common::fixture().await;
    for origin in [
        BROWSER_ORIGIN,
        CUSTOM_ORIGIN,
        VERCEL_ORIGIN,
        "https://foreign.example.test",
        "null",
        "https://convention.enzumeii.com.evil.test",
    ] {
        let response = common::client()
            .await
            .send(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/v1/auth/login")
                    .header("origin", origin)
                    .header("access-control-request-method", "POST")
                    .header(
                        "access-control-request-headers",
                        "content-type,x-session-mode",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if [BROWSER_ORIGIN, CUSTOM_ORIGIN, VERCEL_ORIGIN].contains(&origin) {
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["access-control-allow-origin"], origin);
            assert_eq!(
                response.headers()["access-control-allow-credentials"],
                "true"
            );
            assert!(
                response.headers()["access-control-allow-methods"]
                    .to_str()
                    .unwrap()
                    .split(',')
                    .any(|method| method.trim() == "POST")
            );
            let headers = response.headers()["access-control-allow-headers"]
                .to_str()
                .unwrap()
                .to_ascii_lowercase();
            for header in ["content-type", "x-session-mode"] {
                assert!(headers.split(',').any(|value| value.trim() == header));
            }
        } else {
            assert!(
                !response
                    .headers()
                    .contains_key("access-control-allow-origin")
            );
        }
    }
}

#[tokio::test]
async fn additional_origins_support_cookie_login_rotation_and_logout() {
    let db = common::fixture().await;
    for origin in [CUSTOM_ORIGIN, VERCEL_ORIGIN] {
        let response =
            cookie_login_from_origin(&db.username("root"), "fixture-root-password", origin).await;
        assert_eq!(response.status(), StatusCode::OK, "{origin}");
        assert_eq!(response.headers()["access-control-allow-origin"], origin);
        assert_eq!(
            response.headers()["access-control-allow-credentials"],
            "true"
        );
        assert_eq!(response.headers()["cache-control"], "no-store");
        let mut cookie = cookie_pair(&response);
        let body = common::body_json(response).await;
        assert!(body.get("access_token").is_none());
        assert!(body.get("refresh_token").is_none());
        let old = cookie.clone();
        let mut rotated_cookie = String::new();
        for (path, expected) in [
            ("refresh", StatusCode::OK),
            ("logout", StatusCode::NO_CONTENT),
        ] {
            let response = cookie_request_from_origin(
                "POST",
                &format!("/api/v1/auth/{path}"),
                &cookie,
                origin,
            )
            .await;
            assert_eq!(response.status(), expected, "{origin}: {path}");
            assert_eq!(response.headers()["access-control-allow-origin"], origin);
            assert_eq!(
                response.headers()["access-control-allow-credentials"],
                "true"
            );
            assert_eq!(response.headers()["cache-control"], "no-store");
            cookie = cookie_pair(&response);
            if path == "refresh" {
                assert_ne!(old, cookie);
                rotated_cookie.clone_from(&cookie);
            } else {
                assert!(
                    response.headers()["set-cookie"]
                        .to_str()
                        .unwrap()
                        .contains("Max-Age=0")
                );
            }
        }
        for revoked in [&old, &rotated_cookie] {
            assert_eq!(
                cookie_request("POST", "/api/v1/auth/refresh", revoked)
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
}

#[tokio::test]
async fn cors_without_origin_has_no_allow_origin_header() {
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
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-origin")
    );
}
