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

pub fn stop_process(process: &mut Child, timeout: Duration) {
    if process.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &process.id().to_string()])
            .stderr(Stdio::null())
            .status();
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if process.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(not(unix))]
    let _ = timeout;
    let _ = process.kill();
    let _ = process.wait();
}

pub const JWT_SECRET: &str = "abababababababababababababababababababababababababababababababab";
const FIXTURE_LOCK: i64 = 6_843_819_200_123;

pub struct TestFixture {
    url: String,
    prefix: String,
    pool: PgPool,
    lock: Option<sqlx::PgConnection>,
    root: i64,
    report_ids: std::sync::Mutex<Vec<Uuid>>,
    _api: Arc<TestApi>,
}
impl TestFixture {
    pub fn own_report(&self, id: Uuid) {
        self.report_ids.lock().unwrap().push(id);
    }
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
        report_ids: std::sync::Mutex::new(Vec::new()),
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
        let report_ids = self.report_ids.lock().unwrap().clone();
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
                sqlx::query("DELETE FROM report_results WHERE id = ANY($2) OR id IN (SELECT job.id FROM cron_jobs job JOIN cron_configurations config ON job.cron_configuration_id=config.id WHERE left(config.name,12)=$1)")
                    .bind(&prefix)
                    .bind(&report_ids)
                    .execute(&pool)
                    .await
                    .expect("owned scheduled report cleanup");
                sqlx::query("DELETE FROM cron_jobs WHERE cron_configuration_id IN (SELECT id FROM cron_configurations WHERE left(name,12)=$1)")
                    .bind(&prefix)
                    .execute(&pool)
                    .await
                    .expect("owned scheduler job cleanup");
                sqlx::query("DELETE FROM cron_configurations WHERE left(name,12)=$1")
                    .bind(&prefix)
                    .execute(&pool)
                    .await
                    .expect("owned schedule cleanup");
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
    pub fn storage_base(&self) -> &str {
        &self.api.storage_base
    }
    pub fn shutdown(self) -> (bool, String) {
        let mut api = Arc::try_unwrap(self.api)
            .unwrap_or_else(|_| panic!("shutdown requires an owned test API"));
        let process = api.process.as_mut().unwrap();
        stop_process(process, Duration::from_secs(5));
        let success = process.try_wait().unwrap().unwrap().success();
        (
            success,
            std::fs::read_to_string(api.directory.join("api.log")).unwrap(),
        )
    }
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
    let nodes = serde_json::Value::Array(nodes);
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
    storage_base: String,
    directory: std::path::PathBuf,
    base: String,
}

impl Drop for TestApi {
    fn drop(&mut self) {
        if let Some(process) = &mut self.process {
            stop_process(process, Duration::from_secs(5));
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub fn database_url() -> String {
    std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into())
}

pub fn s3_credentials() -> (String, String) {
    (
        std::env::var("E2E_S3_ACCESS_KEY_ID").unwrap_or_else(|_| "synthetic-access-id".into()),
        std::env::var("E2E_S3_SECRET_ACCESS_KEY").unwrap_or_else(|_| "synthetic-secret-key".into()),
    )
}

pub fn s3_client(endpoint: &str) -> api::client::s3::S3Client {
    let (access_key_id, secret_access_key) = s3_credentials();
    api::client::s3::S3Client::new(&api::config::S3Config {
        endpoint: endpoint.into(),
        region: "ap-southeast-1".into(),
        bucket: "reports".into(),
        force_path_style: true,
        timeout_secs: 3,
        signed_url_ttl_secs: 300,
        access_key_id: Some(access_key_id),
        secret_access_key: Some(secret_access_key),
    })
    .unwrap()
}

async fn test_api() -> Arc<TestApi> {
    let mut shared = API.lock().await;
    if let Some(api) = shared.upgrade() {
        return api;
    }
    let pool = PgPool::connect(&database_url()).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool.close().await;
    let api = start_api(ProviderConfig::default(), None).await;
    *shared = Arc::downgrade(&api);
    api
}

pub async fn client_with_shopify(url: &str) -> TestClient {
    TestClient {
        http: reqwest::Client::new(),
        api: start_api(
            ProviderConfig {
                shopify_url: Some(url),
                ..Default::default()
            },
            None,
        )
        .await,
    }
}

#[derive(Default)]
pub struct ProviderConfig<'a> {
    pub shopify_url: Option<&'a str>,
    pub paypal_url: Option<&'a str>,
    pub stripe_url: Option<&'a str>,
    pub resend_url: Option<&'a str>,
    pub resend_key: Option<&'a str>,
    pub email_timeout_secs: Option<u64>,
    pub cron_secret: Option<&'a str>,
    pub invocation_budget_secs: Option<u64>,
    pub database_url: Option<&'a str>,
    pub vps_cron: bool,
    pub scheduler_enabled: Option<bool>,
    pub poll_interval_secs: Option<u64>,
    pub retention_interval_secs: Option<u64>,
    pub s3_url: Option<&'a str>,
    pub s3_disabled: bool,
    pub s3_ttl_secs: Option<u32>,
    pub s3_invalid_credentials: bool,
}

pub async fn client_with_providers(config: ProviderConfig<'_>) -> TestClient {
    TestClient {
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap(),
        api: start_api(config, None).await,
    }
}

pub async fn client_with_scheduler(
    config: ProviderConfig<'_>,
    retry_backoff_secs: u64,
) -> TestClient {
    TestClient {
        http: reqwest::Client::new(),
        api: start_api(config, Some(retry_backoff_secs)).await,
    }
}

async fn start_api(config: ProviderConfig<'_>, scheduler_backoff: Option<u64>) -> Arc<TestApi> {
    let (mut api, port) = prepare_api(&config, scheduler_backoff);
    assert!(
        reqwest::get(format!("{}health", api.storage_base))
            .await
            .expect("Docker S3 health connection")
            .status()
            .is_success()
    );

    api.process = Some(spawn_api(&api, &config, scheduler_backoff, &port));
    wait_for_api(&mut api).await;
    Arc::new(api)
}

fn prepare_api(config: &ProviderConfig<'_>, scheduler_backoff: Option<u64>) -> (TestApi, String) {
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
    let storage_port = std::env::var("E2E_S3_PORT").unwrap_or_else(|_| "5434".into());
    let directory = std::env::temp_dir().join(format!("monolith-api-e2e-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).expect("create owned E2E configuration directory");
    let api = TestApi {
        process: None,
        storage_base: format!("http://127.0.0.1:{storage_port}/"),
        directory,
        base: format!("http://127.0.0.1:{port}"),
    };
    write_api_configuration(&api, config, scheduler_backoff, &port);
    (api, port)
}

fn write_api_configuration(
    api: &TestApi,
    config: &ProviderConfig<'_>,
    scheduler_backoff: Option<u64>,
    port: &str,
) {
    let shopify_url = config.shopify_url;
    let invocation_budget_secs = config.invocation_budget_secs.unwrap_or(30);
    let scheduler_enabled = config.scheduler_enabled.unwrap_or(true);
    let retry_backoff_secs = scheduler_backoff.unwrap_or(1);
    let shopify = shopify_url
        .map(|url| format!("\n[client.shopify]\nadmin_api_url = {url:?}\ntimeout_secs = 3\n"))
        .unwrap_or_default();
    let stripe = config
        .stripe_url
        .map(|url| format!("\n[client.stripe]\nbase_url = {url:?}\ntimeout_secs = 3\nsecret_key = \"e2e-fixture-key\"\n"))
        .unwrap_or_default();
    let paypal = config
        .paypal_url
        .map(|url| format!("\n[client.paypal]\nbase_url = {url:?}\ntimeout_secs = 3\n"))
        .unwrap_or_default();
    let resend = config
        .email_timeout_secs
        .map(|secs| format!("\n[client.resend]\ntimeout_secs = {secs}\n"))
        .unwrap_or_default();
    let s3_timeout_secs = if config.s3_url.is_some() { 1 } else { 3 };
    let storage_port = std::env::var("E2E_S3_PORT").unwrap_or_else(|_| "5434".into());
    let s3_url = config.s3_url.map_or_else(
        || format!("http://127.0.0.1:{storage_port}/"),
        str::to_owned,
    );
    let s3_ttl_secs = config.s3_ttl_secs.unwrap_or(300);
    let s3 = if config.s3_disabled {
        String::new()
    } else {
        format!(
            r#"
[client.s3]
endpoint = "{s3_url}"
region = "ap-southeast-1"
bucket = "reports"
force_path_style = true
timeout_secs = {s3_timeout_secs}
signed_url_ttl_secs = {s3_ttl_secs}
"#
        )
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
[scheduler]
enabled = {scheduler_enabled}
concurrency = 1
max_attempts = 2
retry_backoff_secs = {retry_backoff_secs}
lease_secs = 5
max_payload_bytes = 20971520
max_email_bytes = 20971520
{s3}
[browser]
public_origin = "http://127.0.0.1:8084"
additional_origins = ["https://convention.enzumeii.com", "https://project.vercel.app"]
insecure_local = true
{shopify}
{stripe}
{paypal}
{resend}
"#
        ),
    )
    .expect("write non-secret E2E configuration");
    let implementation = if config.vps_cron { "vps" } else { "vercel" };
    std::fs::write(
        api.directory.join("application.local.toml"),
        format!(
            "[scheduler]\nimplementation={implementation:?}\npoll_interval_secs={}\nretention_interval_secs={}\ninvocation_budget_secs={invocation_budget_secs}\nretention_reserve_secs=5\n",
            config.poll_interval_secs.unwrap_or(1),
            config.retention_interval_secs.unwrap_or(1)
        ),
    ).unwrap();
}

fn spawn_api(
    api: &TestApi,
    config: &ProviderConfig<'_>,
    scheduler_backoff: Option<u64>,
    port: &str,
) -> Child {
    let shopify_url = config.shopify_url;
    let (access_key_id, secret_access_key) = s3_credentials();
    Command::new(env!("CARGO_BIN_EXE_api"))
        .arg("server")
        .current_dir(&api.directory)
        .env(
            "DATABASE_URL",
            config.database_url.map_or_else(database_url, str::to_owned),
        )
        .env("JWT_SECRET", JWT_SECRET)
        .env(
            "CRON_SECRET",
            config.cron_secret.unwrap_or("e2e-cron-secret"),
        )
        .env_remove("S3_ACCESS_KEY_ID")
        .env_remove("S3_SECRET_ACCESS_KEY")
        .envs(
            (!config.s3_disabled)
                .then_some([
                    (
                        "S3_ACCESS_KEY_ID",
                        if config.s3_invalid_credentials {
                            ""
                        } else if config.s3_url.is_some() {
                            "synthetic-access-id"
                        } else {
                            access_key_id.as_str()
                        },
                    ),
                    (
                        "S3_SECRET_ACCESS_KEY",
                        if config.s3_invalid_credentials {
                            ""
                        } else if config.s3_url.is_some() {
                            "synthetic-secret-key"
                        } else {
                            secret_access_key.as_str()
                        },
                    ),
                ])
                .into_iter()
                .flatten(),
        )
        .env("RUST_LOG", "info")
        .env_remove("SHIPPING_SENDER_NAME")
        .env_remove("SHIPPING_SENDER_PHONE")
        .env_remove("SHIPPING_SENDER_EMAIL")
        .env_remove("SHIPPING_SENDER_ADDRESS")
        .env_remove("SHIPPING_SENDER_SUB_DISTRICT")
        .env_remove("SHIPPING_SENDER_DISTRICT")
        .env_remove("SHIPPING_SENDER_PROVINCE")
        .env_remove("SHIPPING_SENDER_POSTCODE")
        .env_remove("SHIPPING_SENDER_COUNTRY_CODE")
        .envs(
            (scheduler_backoff.is_none())
                .then_some([
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
                .into_iter()
                .flatten(),
        )
        .env_remove("SHOPIFY_ACCESS_TOKEN")
        .env_remove("RESEND_API_KEY")
        .env_remove("RESEND_BASE_URL")
        .env_remove("RESEND_RATE_LIMIT")
        .env("RUN_ENV", "local")
        .env("PORT", port)
        .envs(config.resend_key.map(|key| ("RESEND_API_KEY", key)))
        .envs(config.resend_url.map(|url| ("RESEND_BASE_URL", url)))
        .envs(config.resend_url.map(|_| ("RESEND_RATE_LIMIT", "1000")))
        .env_remove("PAYPAL_CLIENT_ID")
        .env_remove("PAYPAL_CLIENT_SECRET")
        .env_remove("STRIPE_SECRET_KEY")
        .envs(shopify_url.map(|_| ("SHOPIFY_ACCESS_TOKEN", "e2e-fixture-token")))
        .envs(
            config
                .paypal_url
                .map(|_| ("PAYPAL_CLIENT_ID", "e2e-fixture-id")),
        )
        .envs(
            config
                .paypal_url
                .map(|_| ("PAYPAL_CLIENT_SECRET", "e2e-fixture-secret")),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            std::fs::File::create(api.directory.join("api.log")).expect("create owned API log"),
        ))
        .spawn()
        .expect("start local E2E API")
}

async fn wait_for_api(api: &mut TestApi) {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    for _ in 0..60 {
        assert!(
            api.process.as_mut().unwrap().try_wait().unwrap().is_none(),
            "E2E API exited before readiness: {}",
            std::fs::read_to_string(api.directory.join("api.log")).unwrap_or_default()
        );
        if http
            .get(format!("{}/healthz", api.base))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("E2E API did not become ready at {}", api.base);
}

pub struct WireUpstream {
    task: tokio::task::JoinHandle<()>,
    pub requests: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
}
impl Drop for WireUpstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn client_with_wire_script(
    replies: Vec<Option<serde_json::Value>>,
) -> (crate::common::TestClient, WireUpstream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let task = tokio::spawn(async move {
        let mut replies = std::collections::VecDeque::from(replies);
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, length) = loop {
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                    assert!(headers.contains("x-shopify-access-token: e2e-fixture-token"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|n| n.parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            recorded
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap());
            if let Some(reply) = replies
                .pop_front()
                .expect("unexpected extra Shopify request")
            {
                let body = reply.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });
    let client =
        crate::common::client_with_shopify(&format!("http://{address}/graphql.json")).await;
    (client, WireUpstream { task, requests })
}
