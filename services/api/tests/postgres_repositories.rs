#![allow(clippy::iter_filter_is_some, clippy::unwrap_used)]

use api::{
    config::{DatabaseConfig, JwtSettings, ServerConfig, Settings},
    repositories::{
        self,
        admin::{AdminRepositoryTrait, PgAdminRepository},
        auth::{AuthRepositoryTrait, PgAuthRepository},
    },
    state::AppState,
};
use domain::{dto::AdminPageRequest, entities::AdminRole};
use sqlx::PgPool;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, core::IntoContainerPort, runners::AsyncRunner},
};

struct TestDatabase {
    _container: ContainerAsync<Postgres>,
    pool: PgPool,
    connection_url: String,
}

async fn test_database() -> TestDatabase {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432.tcp()).await.unwrap();
    let connection_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&connection_url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    TestDatabase {
        _container: container,
        pool,
        connection_url,
    }
}

async fn unmigrated_test_database() -> (ContainerAsync<Postgres>, String) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432.tcp()).await.unwrap();
    let connection_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, connection_url)
}

#[tokio::test]
async fn admin_pagination_returns_deterministic_bounded_pages() {
    let database = test_database().await;
    let repository = PgAdminRepository::new(database.pool.clone());
    for id in 1..=26 {
        repository
            .add(format!("page-admin-{id}"), "hash".into(), AdminRole::Admin)
            .await
            .unwrap();
    }

    let first_page = repository
        .list_page(AdminPageRequest {
            page: 1,
            per_page: 25,
        })
        .await
        .unwrap();
    let second_page = repository
        .list_page(AdminPageRequest {
            page: 2,
            per_page: 25,
        })
        .await
        .unwrap();

    assert_eq!(first_page.len(), 26);
    assert_eq!(second_page.len(), 2);
    assert!(
        first_page
            .windows(2)
            .all(|admins| admins[0].id < admins[1].id)
    );
    assert_eq!(second_page[0].username, "page-admin-25");
}

#[tokio::test]
async fn refresh_token_is_consumed_once() {
    let database = test_database().await;
    sqlx::query(
        "INSERT INTO refresh_tokens (admin_id, token_hash, expires_at) VALUES (1, 'token-hash', NOW() + INTERVAL '1 hour')",
    )
    .execute(&database.pool)
    .await
    .unwrap();
    let repository = PgAuthRepository::new(database.pool.clone());

    let (first, second) = tokio::join!(
        repository.consume_refresh_token("token-hash"),
        repository.consume_refresh_token("token-hash"),
    );

    let successes = [first.unwrap(), second.unwrap()]
        .into_iter()
        .filter(Option::is_some)
        .count();
    assert_eq!(successes, 1);
}

#[tokio::test]
async fn admin_repository_supports_full_crud_lifecycle() {
    let database = test_database().await;
    let repository = PgAdminRepository::new(database.pool.clone());

    let created = repository
        .add("crud-admin".into(), "hash-1".into(), AdminRole::Admin)
        .await
        .unwrap();
    assert_eq!(created.username, "crud-admin");
    assert_eq!(created.role, AdminRole::Admin);

    let found_by_username = repository
        .find_by_username("crud-admin")
        .await
        .unwrap()
        .expect("admin should be found by username");
    assert_eq!(found_by_username.id, created.id);

    let found_by_id = repository
        .find_by_id(created.id)
        .await
        .unwrap()
        .expect("admin should be found by id");
    assert_eq!(found_by_id.username, "crud-admin");

    let updated = repository
        .update(created.id, Some("crud-admin-renamed".into()), None)
        .await
        .unwrap()
        .expect("update should return the modified row");
    assert_eq!(updated.username, "crud-admin-renamed");
    assert_eq!(updated.password_hash, "hash-1");

    let updated_password = repository
        .update(created.id, None, Some("hash-2".into()))
        .await
        .unwrap()
        .expect("update should return the modified row");
    assert_eq!(updated_password.username, "crud-admin-renamed");
    assert_eq!(updated_password.password_hash, "hash-2");

    let deleted = repository
        .delete(created.id)
        .await
        .unwrap()
        .expect("delete should return the removed row");
    assert_eq!(deleted.id, created.id);

    let missing_after_delete = repository.find_by_id(created.id).await.unwrap();
    assert!(missing_after_delete.is_none());
}

#[tokio::test]
async fn admin_repository_refuses_to_delete_super_admin_rows() {
    let database = test_database().await;
    let repository = PgAdminRepository::new(database.pool.clone());

    let super_admin = repository
        .add("protected-sa".into(), "hash".into(), AdminRole::SuperAdmin)
        .await
        .unwrap();

    let delete_result = repository.delete(super_admin.id).await.unwrap();
    assert!(delete_result.is_none());

    let still_present = repository
        .find_by_id(super_admin.id)
        .await
        .unwrap()
        .expect("super admin row must remain after a refused delete");
    assert_eq!(still_present.role, AdminRole::SuperAdmin);
}

#[tokio::test]
async fn auth_repository_creates_refresh_tokens_and_logout_deletes_them() {
    let database = test_database().await;
    let admin_repository = PgAdminRepository::new(database.pool.clone());
    let admin = admin_repository
        .add("auth-admin".into(), "hash".into(), AdminRole::Admin)
        .await
        .unwrap();

    let auth_repository = PgAuthRepository::new(database.pool.clone());
    auth_repository
        .create_refresh_token(
            admin.id,
            "created-token-hash",
            chrono::Utc::now() + chrono::Duration::days(7),
        )
        .await
        .unwrap();

    let consumed = auth_repository
        .consume_refresh_token("created-token-hash")
        .await
        .unwrap()
        .expect("the freshly created token should be consumable exactly once");
    assert_eq!(consumed.admin_id, admin.id);

    auth_repository
        .create_refresh_token(
            admin.id,
            "second-token-hash",
            chrono::Utc::now() + chrono::Duration::days(7),
        )
        .await
        .unwrap();
    auth_repository
        .delete_all_refresh_tokens(admin.id)
        .await
        .unwrap();

    let after_logout = auth_repository
        .consume_refresh_token("second-token-hash")
        .await
        .unwrap();
    assert!(after_logout.is_none());
}

#[tokio::test]
async fn create_pool_connects_and_runs_migrations() {
    let (_container, connection_url) = unmigrated_test_database().await;
    let settings = Settings {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
        database: DatabaseConfig {
            url: connection_url,
            max_connections: 2,
            min_connections: 0,
        },
        jwt: JwtSettings {
            secret: "test-secret".into(),
        },
    };

    let pool = repositories::create_pool(&settings).await.unwrap();

    let admin_repository = PgAdminRepository::new(pool.clone());
    let admins = admin_repository
        .find_by_username("missing-admin")
        .await
        .unwrap();
    assert!(admins.is_none());
}

#[tokio::test]
async fn app_state_bind_wires_callable_services_from_one_pool() {
    let database = test_database().await;
    let settings = Settings {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
        },
        database: DatabaseConfig {
            url: database.connection_url.clone(),
            max_connections: 2,
            min_connections: 0,
        },
        jwt: JwtSettings {
            secret: "bind-secret".into(),
        },
    };

    let state = AppState::bind(&database.pool, settings);

    // The migrations seed exactly one super-admin row; assert that the
    // list endpoint reads it through AppState::bind's wiring rather than
    // asserting an arbitrary count.
    let page = state
        .admin_service
        .list(AdminPageRequest {
            page: 1,
            per_page: 25,
        })
        .await
        .unwrap();
    assert_eq!(page.admins.len(), 1);
    assert_eq!(page.admins[0].role, AdminRole::SuperAdmin);

    let login_result = state
        .auth_service
        .login(
            state.settings.jwt.secret.as_bytes(),
            domain::dto::LoginRequest {
                username: "missing-admin".into(),
                password: "whatever".into(),
            },
        )
        .await;
    assert!(matches!(
        login_result,
        Err(api::error::AppError::InvalidCredentials)
    ));
}
