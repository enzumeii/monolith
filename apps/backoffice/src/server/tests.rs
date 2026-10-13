use super::*;
use dioxus_server::axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
#[tokio::test]
async fn mutation_rejects_missing_null_and_foreign_origin() {
    let public = std::env::current_exe()
        .expect("test exe")
        .parent()
        .expect("test parent")
        .join("public");
    std::fs::create_dir_all(public).expect("test asset fixture");
    let state = AppState::new(crate::config::Settings::new().expect("test settings"));
    for origin in [None, Some("null"), Some("https://evil.example")] {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header("content-type", "application/json");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        let response = router(state.clone(), crate::App)
            .oneshot(builder.body(Body::from("{}")).expect("request"))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

use axum_fixture::*;
mod axum_fixture {
    use super::*;
    use dioxus_server::axum::{
        Json,
        response::IntoResponse,
        routing::{get, post},
    };
    use domain::dto::{AdminDTO, AdminRole, TokenResponse};
    use http_body_util::BodyExt;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    pub fn access_token() -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        format!(
            "e30.{}.signature",
            URL_SAFE_NO_PAD.encode(br#"{"sub":7,"exp":4102444800}"#)
        )
    }
    pub struct Fixture {
        pub state: AppState,
        pub calls: Arc<AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    pub async fn fixture(logout_status: StatusCode) -> Fixture {
        let calls = Arc::new(AtomicUsize::new(0));
        let login_calls = calls.clone();
        let upstream = Router::new()
            .route(
                "/auth/login",
                post(move || {
                    let calls = login_calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Json(TokenResponse {
                            access_token: access_token(),
                            refresh_token: "a".repeat(64),
                            token_type: "Bearer".into(),
                            expires_in: 600,
                            id: 7,
                        })
                    }
                }),
            )
            .route(
                "/admins/7",
                get(|headers: dioxus_server::axum::http::HeaderMap| async move {
                    assert_eq!(
                        headers
                            .get("authorization")
                            .expect("forwarded access")
                            .to_str()
                            .expect("header text"),
                        format!("Bearer {}", access_token())
                    );
                    Json(AdminDTO {
                        id: 7,
                        username: "operator".into(),
                        role: AdminRole::Admin,
                    })
                }),
            )
            .route(
                "/auth/logout",
                post(move || async move {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    logout_status.into_response()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let task = tokio::spawn(async move {
            dioxus_server::axum::serve(listener, upstream)
                .await
                .expect("fake upstream");
        });
        let mut settings = crate::config::Settings::new().expect("test settings");
        settings.upstream.api_base = format!("http://{address}");
        Fixture {
            state: AppState::new(settings),
            calls,
            task,
        }
    }
    pub fn request(path: &str, body: &serde_json::Value, cookie: Option<&str>) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .header("origin", "http://127.0.0.1:8084");
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        request.body(Body::from(body.to_string())).expect("request")
    }
    pub async fn bytes(response: Response) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .to_vec()
    }
}

#[tokio::test]
async fn login_sets_http_only_cookie_without_exposing_credentials_in_json() {
    let fake = fixture(StatusCode::NO_CONTENT).await;
    let response = router(fake.state.clone(), crate::App)
        .oneshot(request(
            "/api/auth/login",
            &serde_json::json!({"req":{"username":"operator","password":"test-password"}}),
            None,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("cookie")
        .to_str()
        .expect("cookie text");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=604800"));
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .expect("cache policy"),
        "no-store"
    );
    let body = String::from_utf8(bytes(response).await).expect("JSON");
    assert!(!body.contains(&access_token()));
    assert!(!body.contains(&"a".repeat(64)));
    assert!(body.contains("operator"));
    assert_eq!(fake.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_remote_logout_does_not_clear_the_cookie() {
    let fake = fixture(StatusCode::SERVICE_UNAVAILABLE).await;
    let cookie = format!("backoffice_auth={}~{}", access_token(), "a".repeat(64));
    let response = router(fake.state.clone(), crate::App)
        .oneshot(request(
            "/api/auth/logout",
            &serde_json::json!({}),
            Some(&cookie),
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("set-cookie").is_none());
}

#[tokio::test]
async fn completed_remote_logout_expires_cookie() {
    let fake = fixture(StatusCode::NO_CONTENT).await;
    let cookie = format!("backoffice_auth={}~{}", access_token(), "a".repeat(64));
    let response = router(fake.state.clone(), crate::App)
        .oneshot(request(
            "/api/auth/logout",
            &serde_json::json!({}),
            Some(&cookie),
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("set-cookie")
            .expect("clear cookie")
            .to_str()
            .expect("cookie text")
            .contains("Max-Age=0")
    );
}

#[tokio::test]
async fn production_cookie_is_secure_and_host_scoped() {
    let mut fake = fixture(StatusCode::NO_CONTENT).await;
    let mut settings = (*fake.state.settings).clone();
    settings.session.public_origin = "https://backoffice.example".into();
    settings.session.insecure_local = false;
    settings.validate().expect("secure configuration");
    fake.state = AppState::new(settings);
    let mut req = request(
        "/api/auth/login",
        &serde_json::json!({"req":{"username":"operator","password":"test-password"}}),
        None,
    );
    req.headers_mut().insert(
        "origin",
        "https://backoffice.example".parse().expect("origin"),
    );
    let response = router(fake.state.clone(), crate::App)
        .oneshot(req)
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("cookie")
        .to_str()
        .expect("cookie text");
    assert!(cookie.starts_with("__Host-backoffice_auth="));
    assert!(cookie.contains("Secure"));
    assert!(!cookie.contains("Domain="));
}

#[test]
fn insecure_cookie_mode_is_restricted_to_loopback() {
    let mut settings = crate::config::Settings::new().expect("settings");
    settings.session.public_origin = "http://backoffice.example".into();
    assert!(settings.validate().is_err());
    settings.session.insecure_local = false;
    assert!(settings.validate().is_err());
    settings.session.public_origin = "https://backoffice.example/".into();
    assert!(settings.validate().is_err());
}
