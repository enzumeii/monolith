//! Shared e2e harness.
//!
//! Each test connects to the docker-compose-e2e.yml Postgres (`E2E_DATABASE_URL`),
//! creates its own isolated database, runs migrations, and drops it on drop —
//! so `cargo api-e2e` can run tests in parallel without interference.

use api::{
    config::{DatabaseConfig, JwtSettings, ServerConfig, Settings},
    routes,
    state::app_state,
};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{Router, body::Body, http::Request};
use domain::dto::AdminRole;
use http_body_util::BodyExt;
use jsonwebtoken::{EncodingKey, Header, encode};
use sqlx::PgPool;
use uuid::Uuid;

pub const JWT_SECRET: &str = "abababababababababababababababababababababababababababababababab";

fn isolated_database_name() -> String {
    format!("e2e_test_{}", Uuid::new_v4().simple())
}

pub struct TestDatabase {
    admin_url: String,
    db_name: String,
    pool: PgPool,
}

impl TestDatabase {
    /// A cloned handle to the test database pool (`PgPool` is Arc-based, cheap to clone).
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }
}

/// Connect to the e2e Postgres, create a fresh isolated database, run migrations.
pub async fn test_database() -> TestDatabase {
    let admin_url = std::env::var("E2E_DATABASE_URL")
        .expect("E2E_DATABASE_URL must be set (see .cargo/config.toml)");
    let admin_pool = PgPool::connect(&admin_url).await.unwrap();

    let db_name = isolated_database_name();
    // db_name is a controlled test identifier; identifiers can't be bind params.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db_name}")))
        .execute(&admin_pool)
        .await
        .unwrap();
    drop(admin_pool);

    let test_url = test_url_for(&admin_url, &db_name);
    let pool = PgPool::connect_lazy(&test_url).unwrap();
    let database = TestDatabase {
        admin_url,
        db_name,
        pool,
    };
    sqlx::migrate!("./migrations")
        .run(&database.pool)
        .await
        .unwrap();
    database
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        // Drop can't await; spawn a thread with its own runtime to drop the DB.
        let admin_url = self.admin_url.clone();
        let db_name = self.db_name.clone();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                return;
            };
            rt.block_on(async move {
                if let Ok(pool) = PgPool::connect(&admin_url).await {
                    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                        "DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"
                    )))
                    .execute(&pool)
                    .await;
                }
            });
        })
        .join()
        .ok();
    }
}

fn test_url_for(admin_url: &str, db_name: &str) -> String {
    let last_slash = admin_url
        .rfind('/')
        .expect("E2E_DATABASE_URL has a db path");
    format!("{}{}", &admin_url[..=last_slash], db_name)
}

pub fn router(pool: PgPool) -> Router {
    routes::router(app_state(pool, settings()).unwrap())
}

fn settings() -> Settings {
    Settings {
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
            secret: JWT_SECRET.into(),
        },
    }
}

pub fn hash_password(raw_password: &str) -> String {
    Argon2::default()
        .hash_password(raw_password.as_bytes(), &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string()
}

/// Insert an admin row and return its id.
pub async fn seed_admin(pool: &PgPool, username: &str, password: &str, role: AdminRole) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO admin (username, password_hash, role) VALUES ($1, $2, $3::ADMIN_ROLE) RETURNING id",
    )
    .bind(username)
    .bind(hash_password(password))
    .bind(role.to_string())
    .fetch_one(pool)
    .await
    .unwrap()
}

pub fn jwt_for(admin_id: i64, role: AdminRole) -> String {
    encode(
        &Header::default(),
        &api::auth::Claims {
            admin_id,
            username: "admin".into(),
            role,
            expiration: 4_102_444_800,
        },
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

#[test]
fn independent_runs_have_distinct_database_names() {
    let first = isolated_database_name();
    let second = isolated_database_name();
    assert_ne!(first, second);
    assert!(first.starts_with("e2e_test_"));
    assert!(first.len() < 63);
    assert!(first.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
}
