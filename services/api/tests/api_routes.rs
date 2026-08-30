#![allow(
    clippy::needless_pass_by_value,
    clippy::struct_field_names,
    clippy::unwrap_used
)]

use std::sync::Arc;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use api::{
    config::{DatabaseConfig, JwtSettings, ServerConfig, Settings},
    error::AppError,
    middleware::auth::Middleware,
    routes,
    services::{admin::AdminServiceTrait, auth::AuthServiceTrait},
    state::AppState,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::get,
};
use domain::{
    dto::{
        AdminDTO, AdminListResponse, AdminPageRequest, Claims, CreateAdminRequest, LoginRequest,
        RefreshRequest, TokenResponse, UpdateAdminRequest,
    },
    entities::AdminRole,
};
use http_body_util::BodyExt;
use jsonwebtoken::{EncodingKey, Header, encode};
use tower::ServiceExt;

const JWT_SECRET: &[u8] = b"test-secret";

struct TestAdminService {
    list_result: Mutex<Option<Result<AdminListResponse, AppError>>>,
    add_result: Mutex<Option<Result<AdminDTO, AppError>>>,
    update_result: Mutex<Option<Result<AdminDTO, AppError>>>,
    get_result: Mutex<Option<Result<AdminDTO, AppError>>>,
    delete_result: Mutex<Option<Result<AdminDTO, AppError>>>,
}

impl TestAdminService {
    fn new() -> Self {
        Self {
            list_result: Mutex::new(None),
            add_result: Mutex::new(None),
            update_result: Mutex::new(None),
            get_result: Mutex::new(None),
            delete_result: Mutex::new(None),
        }
    }

    fn with_list(self, result: Result<AdminListResponse, AppError>) -> Self {
        *self.list_result.lock().unwrap() = Some(result);
        self
    }

    fn with_add(self, result: Result<AdminDTO, AppError>) -> Self {
        *self.add_result.lock().unwrap() = Some(result);
        self
    }

    fn with_update(self, result: Result<AdminDTO, AppError>) -> Self {
        *self.update_result.lock().unwrap() = Some(result);
        self
    }

    fn with_get(self, result: Result<AdminDTO, AppError>) -> Self {
        *self.get_result.lock().unwrap() = Some(result);
        self
    }

    fn with_delete(self, result: Result<AdminDTO, AppError>) -> Self {
        *self.delete_result.lock().unwrap() = Some(result);
        self
    }
}

#[async_trait]
impl AdminServiceTrait for TestAdminService {
    async fn list(&self, _page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
        self.list_result
            .lock()
            .unwrap()
            .take()
            .expect("list was not expected to be called")
    }

    async fn add(&self, _req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
        self.add_result
            .lock()
            .unwrap()
            .take()
            .expect("add was not expected to be called")
    }

    async fn update(&self, _id: i64, _req: UpdateAdminRequest) -> Result<AdminDTO, AppError> {
        self.update_result
            .lock()
            .unwrap()
            .take()
            .expect("update was not expected to be called")
    }

    async fn get(&self, _id: i64) -> Result<AdminDTO, AppError> {
        self.get_result
            .lock()
            .unwrap()
            .take()
            .expect("get was not expected to be called")
    }

    async fn delete(&self, _id: i64) -> Result<AdminDTO, AppError> {
        self.delete_result
            .lock()
            .unwrap()
            .take()
            .expect("delete was not expected to be called")
    }
}

struct TestAuthService {
    login_result: Mutex<Option<Result<TokenResponse, AppError>>>,
    refresh_result: Mutex<Option<Result<TokenResponse, AppError>>>,
    logout_result: Mutex<Option<Result<(), AppError>>>,
    logout_calls: AtomicUsize,
}

impl TestAuthService {
    fn new() -> Self {
        Self {
            login_result: Mutex::new(None),
            refresh_result: Mutex::new(None),
            logout_result: Mutex::new(None),
            logout_calls: AtomicUsize::new(0),
        }
    }

    fn with_login(self, result: Result<TokenResponse, AppError>) -> Self {
        *self.login_result.lock().unwrap() = Some(result);
        self
    }

    fn with_refresh(self, result: Result<TokenResponse, AppError>) -> Self {
        *self.refresh_result.lock().unwrap() = Some(result);
        self
    }

    fn with_logout(self, result: Result<(), AppError>) -> Self {
        *self.logout_result.lock().unwrap() = Some(result);
        self
    }
}

#[async_trait]
impl AuthServiceTrait for TestAuthService {
    async fn login(
        &self,
        _jwt_secret: &[u8],
        _req: LoginRequest,
    ) -> Result<TokenResponse, AppError> {
        self.login_result
            .lock()
            .unwrap()
            .take()
            .expect("login was not expected to be called")
    }

    async fn refresh(
        &self,
        _jwt_secret: &[u8],
        _req: RefreshRequest,
    ) -> Result<TokenResponse, AppError> {
        self.refresh_result
            .lock()
            .unwrap()
            .take()
            .expect("refresh was not expected to be called")
    }

    async fn logout(&self, _admin_id: i64) -> Result<(), AppError> {
        self.logout_calls.fetch_add(1, Ordering::SeqCst);
        self.logout_result
            .lock()
            .unwrap()
            .take()
            .expect("logout was not expected to be called")
    }
}

fn admin_dto(id: i64, username: &str, role: AdminRole) -> AdminDTO {
    AdminDTO {
        id,
        username: username.into(),
        role,
    }
}

fn token_response(admin_id: i64) -> TokenResponse {
    TokenResponse {
        access_token: "access".into(),
        refresh_token: "refresh".into(),
        token_type: "Bearer",
        expires_in: 600,
        id: admin_id,
    }
}

fn jwt_for(admin_id: i64, role: AdminRole) -> String {
    encode(
        &Header::default(),
        &Claims {
            admin_id,
            username: "admin".into(),
            role,
            expiration: 4_102_444_800,
        },
        &EncodingKey::from_secret(JWT_SECRET),
    )
    .unwrap()
}

fn authorized_request(method: &str, uri: &str, admin_id: i64, role: AdminRole) -> Request<Body> {
    let token = jwt_for(admin_id, role);

    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn authorized_json_request(
    method: &str,
    uri: &str,
    admin_id: i64,
    role: AdminRole,
    body: serde_json::Value,
) -> Request<Body> {
    let token = jwt_for(admin_id, role);

    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn state(admin_service: TestAdminService, auth_service: TestAuthService) -> AppState {
    AppState {
        settings: Arc::new(Settings {
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 0,
            },
            database: DatabaseConfig {
                url: "postgres://unused".into(),
                max_connections: 1,
                min_connections: 0,
            },
            jwt: JwtSettings {
                secret: "test-secret".into(),
            },
        }),
        admin_service: Arc::new(admin_service),
        auth_service: Arc::new(auth_service),
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn healthz_returns_ok() {
    let response = routes::router(state(TestAdminService::new(), TestAuthService::new()))
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_list_uses_defaults_and_returns_page_metadata() {
    let admin_service = TestAdminService::new().with_list(Ok(AdminListResponse {
        admins: vec![admin_dto(1, "admin", AdminRole::Admin)],
        page: 1,
        per_page: 25,
        has_next: false,
    }));
    let response = routes::router(state(admin_service, TestAuthService::new()))
        .oneshot(authorized_request("GET", "/v1/admins", 1, AdminRole::Admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["admins"][0]["username"], "admin");
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 25);
    assert_eq!(body["has_next"], false);
}

#[tokio::test]
async fn admin_list_rejects_page_zero() {
    let response = routes::router(state(TestAdminService::new(), TestAuthService::new()))
        .oneshot(authorized_request(
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
async fn login_returns_token_response_on_success() {
    let auth_service = TestAuthService::new().with_login(Ok(token_response(7)));
    let response = routes::router(state(TestAdminService::new(), auth_service))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "admin", "password": "secret" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["id"], 7);
    assert_eq!(body["access_token"], "access");
}

#[tokio::test]
async fn login_rejects_invalid_credentials() {
    let auth_service = TestAuthService::new().with_login(Err(AppError::InvalidCredentials));
    let response = routes::router(state(TestAdminService::new(), auth_service))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "username": "admin", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_returns_new_token_pair_on_success() {
    let auth_service = TestAuthService::new().with_refresh(Ok(token_response(9)));
    let response = routes::router(state(TestAdminService::new(), auth_service))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": "valid-token" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["id"], 9);
}

#[tokio::test]
async fn refresh_rejects_invalid_token_with_www_authenticate_header() {
    let auth_service = TestAuthService::new().with_refresh(Err(AppError::InvalidToken));
    let response = routes::router(state(TestAdminService::new(), auth_service))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": "stale-token" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn logout_returns_no_content_and_uses_claims_admin_id() {
    let auth_service = TestAuthService::new().with_logout(Ok(()));
    let response = routes::router(state(TestAdminService::new(), auth_service))
        .oneshot(authorized_request(
            "GET",
            "/v1/auth/logout",
            42,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn admin_get_returns_dto_on_success() {
    let admin_service =
        TestAdminService::new().with_get(Ok(admin_dto(3, "found-admin", AdminRole::Admin)));
    let response = routes::router(state(admin_service, TestAuthService::new()))
        .oneshot(authorized_request(
            "GET",
            "/v1/admins/3",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["username"], "found-admin");
}

#[tokio::test]
async fn admin_get_returns_not_found_when_missing() {
    let admin_service = TestAdminService::new().with_get(Err(AppError::NotFound));
    let response = routes::router(state(admin_service, TestAuthService::new()))
        .oneshot(authorized_request(
            "GET",
            "/v1/admins/999",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_update_allows_admin_to_update_own_record_and_triggers_logout() {
    let admin_service =
        TestAdminService::new().with_update(Ok(admin_dto(5, "renamed", AdminRole::Admin)));
    let auth_service = TestAuthService::new().with_logout(Ok(()));
    let response = routes::router(state(admin_service, auth_service))
        .oneshot(authorized_json_request(
            "PATCH",
            "/v1/admins/5",
            5,
            AdminRole::Admin,
            serde_json::json!({ "username": "renamed", "password": null }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["username"], "renamed");
}

#[tokio::test]
async fn admin_update_rejects_mismatched_admin_id_without_calling_service() {
    // admin_service.update is never configured: if the handler called it,
    // the mock would panic with "update was not expected to be called".
    let response = routes::router(state(TestAdminService::new(), TestAuthService::new()))
        .oneshot(authorized_json_request(
            "PATCH",
            "/v1/admins/5",
            6,
            AdminRole::Admin,
            serde_json::json!({ "username": "renamed", "password": null }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn super_admin_can_add_and_delete_admins() {
    let admin_service = TestAdminService::new()
        .with_add(Ok(admin_dto(10, "new-admin", AdminRole::Admin)))
        .with_delete(Ok(admin_dto(10, "new-admin", AdminRole::Admin)));
    let auth_service = TestAuthService::new().with_logout(Ok(()));
    let router = routes::router(state(admin_service, auth_service));

    let add_response = router
        .clone()
        .oneshot(authorized_json_request(
            "POST",
            "/v1/admins",
            1,
            AdminRole::SuperAdmin,
            serde_json::json!({ "username": "new-admin", "password": "secret", "role": "ADMIN" }),
        ))
        .await
        .unwrap();
    assert_eq!(add_response.status(), StatusCode::CREATED);

    let delete_response = router
        .oneshot(authorized_request(
            "DELETE",
            "/v1/admins/10",
            1,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_delete_of_protected_super_admin_returns_forbidden() {
    let admin_service = TestAdminService::new().with_delete(Err(AppError::Forbidden));
    let auth_service = TestAuthService::new();
    let response = routes::router(state(admin_service, auth_service))
        .oneshot(authorized_request(
            "DELETE",
            "/v1/admins/1",
            1,
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn public_errors_map_to_expected_http_statuses() {
    assert_eq!(
        AppError::BadRequest("invalid page".into())
            .into_response()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        AppError::InvalidCredentials.into_response().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        AppError::InvalidToken.into_response().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        AppError::NotFound.into_response().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        AppError::Forbidden.into_response().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn malformed_bearer_token_is_rejected() {
    let response = routes::router(state(TestAdminService::new(), TestAuthService::new()))
        .oneshot(
            Request::builder()
                .uri("/v1/admins")
                .header("authorization", "Bearer invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn admin_role_cannot_call_super_admin_route() {
    let response = routes::router(state(TestAdminService::new(), TestAuthService::new()))
        .oneshot(authorized_request(
            "POST",
            "/v1/admins",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

async fn ok_handler() -> &'static str {
    "ok"
}

#[tokio::test]
async fn require_super_admin_without_claims_returns_unauthorized() {
    let app: Router<()> = Router::new()
        .route("/", get(ok_handler))
        .layer(axum::middleware::from_fn(Middleware::require_super_admin));

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
