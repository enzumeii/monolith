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
            let rt=tokio::runtime::Runtime::new().expect("cleanup runtime");
            rt.block_on(async move {
                let pool=PgPool::connect(&url).await.expect("cleanup database");
                // Serialized trigger tests use only their own rows; remove their controls before cleanup.
                for (trigger,function) in [("reject_refresh","reject_refresh_insert"),("reject_revoke","reject_revocation"),("pause_refresh","pause_refresh")] {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER IF EXISTS {prefix}_{trigger} ON refresh_tokens; DROP FUNCTION IF EXISTS {prefix}_{function}();"))).execute(&pool).await.expect("owned trigger cleanup");
                }
                sqlx::query("DELETE FROM admin WHERE left(username,12)=$1").bind(&prefix).execute(&pool).await.expect("owned row cleanup");
                pool.close().await;
                drop(lock);
            });
        }).join().expect("fixture cleanup");
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
    pub async fn oneshot(
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

struct TestApi {
    process: Child,
    base: String,
}

impl Drop for TestApi {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
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
    let port = std::env::var("E2E_API_PORT").unwrap_or_else(|_| "3100".into());
    let mut api = TestApi {
        process: Command::new(env!("CARGO_BIN_EXE_api"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("RUN_ENV", "local")
            .env("DATABASE_URL", database_url())
            .env("JWT_SECRET", JWT_SECRET)
            .env("API_HOST", "127.0.0.1")
            .env("API_PORT", &port)
            .env("PUBLIC_ORIGIN", "http://127.0.0.1:8084")
            .env("INSECURE_LOCAL_COOKIES", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .expect("start local E2E API"),
        base: format!("http://127.0.0.1:{port}"),
    };
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    for _ in 0..60 {
        assert!(
            api.process.try_wait().unwrap().is_none(),
            "E2E API exited before readiness"
        );
        if http
            .get(format!("{}/healthz", api.base))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            let api = Arc::new(api);
            *shared = Arc::downgrade(&api);
            return api;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("E2E API did not become ready at {}", api.base);
}
