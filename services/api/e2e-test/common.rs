//! Shared e2e harness.
//!
//! Each test connects to the docker-compose-e2e.yml Postgres (`E2E_DATABASE_URL`),
//! owns a unique fixture prefix and cleans up only its own records.
//! Advisory locking serializes whole-table and controlled trigger scenarios across runs.

use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{body::Body, http::Request};
use domain::dto::AdminRole;
use http_body_util::BodyExt;
use jsonwebtoken::{EncodingKey, Header, encode};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgPool};
use std::{
    process::{Child, Command, Stdio},
    sync::{Arc, Weak},
    time::Duration,
};
use uuid::Uuid;

static API: tokio::sync::Mutex<Weak<TestApi>> = tokio::sync::Mutex::const_new(Weak::new());

pub const JWT_SECRET: &str = "abababababababababababababababababababababababababababababababab";
const FIXTURE_LOCK: i64 = 6_843_819_200_123;

pub struct TestFixture {
    url: String,
    prefix: String,
    pool: PgPool,
    lock: Option<sqlx::PgConnection>,
    root: i64,
    _api: Arc<TestApi>,
}
impl TestFixture {
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }
    pub fn root_id(&self) -> i64 {
        self.root
    }
    pub fn username(&self, label: &str) -> String {
        format!(
            "{}{}",
            self.prefix,
            &hex::encode(Sha256::digest(label.as_bytes()))[..8]
        )
    }
}
pub async fn fixture() -> TestFixture {
    let api = test_api().await;
    let url = database_url();
    let mut lock = sqlx::PgConnection::connect(&url).await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(FIXTURE_LOCK)
        .execute(&mut lock)
        .await
        .unwrap();
    let mut fixture = TestFixture {
        pool: PgPool::connect(&url).await.unwrap(),
        url,
        prefix: format!("e{}", &Uuid::new_v4().simple().to_string()[..11]),
        lock: Some(lock),
        root: 0,
        _api: api,
    };
    fixture.root = seed_admin(
        &fixture,
        fixture.username("root"),
        "fixture-root-password",
        AdminRole::SuperAdmin,
    )
    .await;
    fixture
}
impl Drop for TestFixture {
    fn drop(&mut self) {
        let url = self.url.clone();
        let prefix = self.prefix.clone();
        let lock = self.lock.take();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("cleanup runtime");
            runtime.block_on(async move {
                let pool = PgPool::connect(&url).await.expect("cleanup database");
                // Remove only this fixture's race controls before deleting its rows.
                for (trigger, function) in [
                    ("reject_refresh", "reject_refresh_insert"),
                    ("reject_revoke", "reject_revocation"),
                    ("pause_refresh", "pause_refresh"),
                ] {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                        "DROP TRIGGER IF EXISTS {prefix}_{trigger} ON refresh_tokens; \
                         DROP FUNCTION IF EXISTS {prefix}_{function}();"
                    )))
                    .execute(&pool)
                    .await
                    .expect("owned trigger cleanup");
                }
                sqlx::query("DELETE FROM admin WHERE left(username,12)=$1")
                    .bind(&prefix)
                    .execute(&pool)
                    .await
                    .expect("owned row cleanup");
                pool.close().await;
                drop(lock);
            });
        })
        .join()
        .expect("fixture cleanup");
    }
}
#[derive(Clone)]
pub struct TestClient {
    http: reqwest::Client,
    api: Arc<TestApi>,
}
pub async fn client() -> TestClient {
    TestClient {
        http: reqwest::Client::new(),
        api: test_api().await,
    }
}
impl TestClient {
    pub fn logs(&self) -> String {
        std::fs::read_to_string(self.api.directory.join("api.log")).unwrap()
    }
    pub async fn send(
        &self,
        request: Request<Body>,
    ) -> Result<axum::response::Response, reqwest::Error> {
        let (parts, body) = request.into_parts();
        let bytes = body.collect().await.expect("test request").to_bytes();
        let response = self
            .http
            .request(parts.method, format!("{}{}", self.api.base, parts.uri))
            .headers(parts.headers)
            .body(bytes)
            .send()
            .await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        let mut response = axum::response::Response::new(Body::from(body));
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        Ok(response)
    }
}
pub fn hash_password(raw_password: &str) -> String {
    Argon2::default()
        .hash_password(raw_password.as_bytes(), &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string()
}

/// Insert an admin row and return its id.
pub async fn seed_admin(
    fixture: &TestFixture,
    username: impl AsRef<str>,
    password: &str,
    role: AdminRole,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO admin (username, password_hash, role) VALUES ($1, $2, $3::ADMIN_ROLE) RETURNING id",
    )
    .bind(username.as_ref())
    .bind(hash_password(password))
    .bind(role.to_string())
    .fetch_one(&fixture.pool)
    .await
    .unwrap()
}

pub fn jwt_for(admin_id: i64, role: AdminRole) -> String {
    encode(
        &Header::default(),
        &serde_json::json!({"sub":admin_id,"username":"fixture","role":role,"exp":4_102_444_800_u64}),
        &EncodingKey::from_secret(&hex::decode(JWT_SECRET).unwrap()),
    )
    .unwrap()
}

pub fn authed(method: &str, uri: &str, admin_id: i64, role: AdminRole) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            "authorization",
            format!("Bearer {}", jwt_for(admin_id, role)),
        )
        .body(Body::empty())
        .unwrap()
}

pub fn authed_json(
    method: &str,
    uri: &str,
    admin_id: i64,
    role: AdminRole,
    body: &serde_json::Value,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            "authorization",
            format!("Bearer {}", jwt_for(admin_id, role)),
        )
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

pub async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

pub fn shopify_page(nodes: Vec<serde_json::Value>, next: Option<&str>) -> serde_json::Value {
    serde_json::json!({"nodes":nodes,"pageInfo":{"hasNextPage":next.is_some(),"endCursor":next}})
}

pub async fn assert_report_auth(client: &TestClient, uri: &str) {
    for token in [None, Some("Bearer invalid")] {
        let mut request = Request::builder().uri(uri);
        if let Some(token) = token {
            request = request.header("authorization", token);
        }
        let response = client
            .send(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_report_error(response, 401, "Invalid or expired token").await;
    }
}

pub async fn assert_report_error(response: axum::response::Response, status: u16, message: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        response
            .headers()
            .get("www-authenticate")
            .map(|value| value.to_str().unwrap()),
        (status == 401).then_some("Bearer")
    );
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        body_json(response).await,
        serde_json::json!({"error":message})
    );
}

pub async fn assert_report_unavailable(
    client: &TestClient,
    uri: &str,
    admin_id: i64,
    role: AdminRole,
) {
    let response = client
        .send(authed("GET", uri, admin_id, role))
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body = body_json(response).await;
    assert_eq!(body["error"], "Shopify client unavailable");
    assert!(body.get("products").is_none());
    assert!(!body.to_string().contains("invalid-url"));
}

pub async fn client_with_upstream(router: axum::Router) -> (TestClient, TestUpstream) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let upstream = TestUpstream(server);
    let client = client_with_shopify(&format!("http://{address}/graphql.json")).await;
    (client, upstream)
}

pub struct ScriptedUpstream {
    requests: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    _server: TestUpstream,
}
impl ScriptedUpstream {
    pub fn requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().unwrap().clone()
    }
}

pub async fn client_with_upstream_script(
    prefix: Vec<serde_json::Value>,
    failure: &'static str,
) -> (TestClient, ScriptedUpstream) {
    let (url, upstream) = scripted_upstream(prefix, failure).await;
    (client_with_shopify(&url).await, upstream)
}

pub async fn scripted_upstream(
    prefix: Vec<serde_json::Value>,
    failure: &'static str,
) -> (String, ScriptedUpstream) {
    use axum::{Json, response::IntoResponse};
    use serde_json::{Value, json};

    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let replies = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        prefix,
    )));
    let router = axum::Router::new().route(
        "/graphql.json",
        axum::routing::post(move |headers: axum::http::HeaderMap, Json(request): Json<Value>| {
            let recorded = recorded.clone();
            let replies = replies.clone();
            async move {
                assert_eq!(headers["x-shopify-access-token"], "e2e-fixture-token");
                recorded.lock().unwrap().push(request);
                let reply = replies.lock().unwrap().pop_front();
                if let Some(reply) = reply {
                    return Json(json!({"data":reply})).into_response();
                }
                match failure {
                    "http" => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "private upstream body").into_response(),
                    "graphql" => Json(json!({"errors":[{"message":"private upstream error"}]})).into_response(),
                    "partial" => Json(json!({"data":{},"errors":[{"message":"private partial result"}]})).into_response(),
                    "malformed" => "{".into_response(),
                    "missing" => Json(json!({"data":null})).into_response(),
                    "shape" => Json(json!({"data":{"orders":false,"location":false,"products":false}})).into_response(),
                    "throttled" => Json(json!({"errors":[{"message":"private throttle","extensions":{"code":"THROTTLED"}}]})).into_response(),
                    "timeout" => {
                        tokio::time::sleep(Duration::from_secs(4)).await;
                        Json(json!({"data":null})).into_response()
                    }
                    _ => panic!("unexpected upstream failure {failure}"),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/graphql.json", listener.local_addr().unwrap());
    let server = TestUpstream(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    (
        url,
        ScriptedUpstream {
            requests,
            _server: server,
        },
    )
}

pub async fn client_with_unreachable_shopify() -> TestClient {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = client_with_shopify(&format!(
        "http://{}/graphql.json",
        listener.local_addr().unwrap()
    ))
    .await;
    drop(listener);
    client
}

pub struct TestUpstream(tokio::task::JoinHandle<()>);
impl Drop for TestUpstream {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct TestApi {
    process: Option<Child>,
    directory: std::path::PathBuf,
    base: String,
}

impl Drop for TestApi {
    fn drop(&mut self) {
        if let Some(process) = &mut self.process {
            let _ = process.kill();
            let _ = process.wait();
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn database_url() -> String {
    std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into())
}

async fn test_api() -> Arc<TestApi> {
    let mut shared = API.lock().await;
    if let Some(api) = shared.upgrade() {
        return api;
    }
    let api = start_api(ProviderConfig::default()).await;
    *shared = Arc::downgrade(&api);
    api
}

pub async fn client_with_shopify(url: &str) -> TestClient {
    TestClient {
        http: reqwest::Client::new(),
        api: start_api(ProviderConfig {
            shopify_url: Some(url),
            ..Default::default()
        })
        .await,
    }
}

#[derive(Default)]
pub struct ProviderConfig<'a> {
    pub shopify_url: Option<&'a str>,
    pub resend_url: Option<&'a str>,
    pub resend_key: Option<&'a str>,
    pub email_timeout_secs: Option<u64>,
}

pub async fn client_with_providers(config: ProviderConfig<'_>) -> TestClient {
    TestClient {
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap(),
        api: start_api(config).await,
    }
}

async fn start_api(config: ProviderConfig<'_>) -> Arc<TestApi> {
    let shopify_url = config.shopify_url;
    let port = if shopify_url.is_some()
        || config.resend_url.is_some()
        || std::env::var("E2E_API_PORT").is_err()
    {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("allocate API port");
        listener
            .local_addr()
            .expect("API address")
            .port()
            .to_string()
    } else {
        std::env::var("E2E_API_PORT").unwrap_or_else(|_| "3100".into())
    };
    let shopify = shopify_url
        .map(|url| format!("\n[client.shopify]\nadmin_api_url = {url:?}\ntimeout_secs = 3\n"))
        .unwrap_or_default();
    let resend = config
        .email_timeout_secs
        .map(|secs| format!("\n[client.resend]\ntimeout_secs = {secs}\n"))
        .unwrap_or_default();
    let directory = std::env::temp_dir().join(format!("monolith-api-e2e-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).expect("create owned E2E configuration directory");
    let mut api = TestApi {
        process: None,
        directory,
        base: format!("http://127.0.0.1:{port}"),
    };
    std::fs::write(
        api.directory.join("application.toml"),
        format!(
            r#"
[server]
host = "127.0.0.1"
port = {port}
[database]
max_connections = 1
min_connections = 1
[browser]
public_origin = "http://127.0.0.1:8084"
insecure_local = true
{shopify}
{resend}
"#
        ),
    )
    .expect("write non-secret E2E configuration");
    api.process = Some(
        Command::new(env!("CARGO_BIN_EXE_api"))
            .current_dir(&api.directory)
            .env("DATABASE_URL", database_url())
            .env("JWT_SECRET", JWT_SECRET)
            .env("RUST_LOG", "info")
            .envs([
                ("SHIPPING_SENDER_NAME", "Synthetic Sender"),
                ("SHIPPING_SENDER_PHONE", "0000000000"),
                ("SHIPPING_SENDER_EMAIL", "sender@example.test"),
                ("SHIPPING_SENDER_ADDRESS", "123 Synthetic Street"),
                ("SHIPPING_SENDER_SUB_DISTRICT", "Synthetic Subdistrict"),
                ("SHIPPING_SENDER_DISTRICT", "Synthetic District"),
                ("SHIPPING_SENDER_PROVINCE", "Synthetic Province"),
                ("SHIPPING_SENDER_POSTCODE", "10100"),
                ("SHIPPING_SENDER_COUNTRY_CODE", "TH"),
            ])
            .env_remove("SHOPIFY_ACCESS_TOKEN")
            .env_remove("RESEND_API_KEY")
            .env_remove("RESEND_BASE_URL")
            .env_remove("RESEND_RATE_LIMIT")
            .env_remove("RUN_ENV")
            .envs(config.resend_key.map(|key| ("RESEND_API_KEY", key)))
            .envs(config.resend_url.map(|url| ("RESEND_BASE_URL", url)))
            .envs(config.resend_url.map(|_| ("RESEND_RATE_LIMIT", "1000")))
            .envs(shopify_url.map(|_| ("SHOPIFY_ACCESS_TOKEN", "e2e-fixture-token")))
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                std::fs::File::create(api.directory.join("api.log")).expect("create owned API log"),
            ))
            .spawn()
            .expect("start local E2E API"),
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    for _ in 0..60 {
        assert!(
            api.process.as_mut().unwrap().try_wait().unwrap().is_none(),
            "E2E API exited before readiness"
        );
        if http
            .get(format!("{}/healthz", api.base))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            let api = Arc::new(api);
            return api;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("E2E API did not become ready at {}", api.base);
}
