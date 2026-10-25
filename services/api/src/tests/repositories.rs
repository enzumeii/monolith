#![allow(clippy::unwrap_used)]

use sqlx::{Connection, PgConnection, postgres::PgPoolOptions};
use uuid::Uuid;

#[tokio::test]
async fn consolidated_migrations_create_a_fresh_schema_and_rerun_without_changes() {
    exercise_migrations(false).await;
}

#[tokio::test]
async fn consolidated_checkout_upgrade_preserves_legacy_rows_and_recovery_eligibility() {
    exercise_migrations(true).await;
}

async fn exercise_migrations(upgrade: bool) {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    migrator.run(&pool).await.unwrap();
    let mut transaction = pool.begin().await.unwrap();
    let schema = format!("migration_regression_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('search_path', $1, true)")
        .bind(&schema)
        .execute(&mut *transaction)
        .await
        .unwrap();
    if upgrade {
        migrator
            .run_to(20_261_024_000_000, &mut *transaction)
            .await
            .unwrap();
        insert_legacy_checkouts(&mut transaction).await;
    }
    migrator.run(&mut *transaction).await.unwrap();
    migrator.run(&mut *transaction).await.unwrap();
    let versions: Vec<i64> = sqlx::query_scalar(
        "SELECT version
         FROM _sqlx_migrations
         WHERE success
         ORDER BY version",
    )
    .fetch_all(&mut *transaction)
    .await
    .unwrap();
    assert_eq!(
        versions,
        [
            20_260_815_172_632,
            20_260_815_174_023,
            20_260_829_090_958,
            20_261_020_100_000,
            20_261_023_090_000,
            20_261_024_000_000,
            20_261_024_010_000,
        ]
    );
    if upgrade {
        assert_legacy_checkouts(&mut transaction).await;
        assert_checkout_constraints(&mut transaction).await;
    }
    let index: String = sqlx::query_scalar(
        "SELECT indexdef
         FROM pg_indexes
         WHERE schemaname = $1
           AND indexname = 'checkouts_recovery_due'",
    )
    .bind(&schema)
    .fetch_one(&mut *transaction)
    .await
    .unwrap();
    for predicate in [
        "next_recovery_at IS NOT NULL",
        "NOT recovery_manual",
        "completion_state <> 'completed'",
        "payment_state <> 'unpaid'",
    ] {
        assert!(index.contains(predicate), "{index}");
    }
    transaction.rollback().await.unwrap();
    let remains: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM pg_namespace
             WHERE nspname = $1
         )",
    )
    .bind(schema)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!remains);
    pool.close().await;
}

async fn insert_legacy_checkouts(connection: &mut PgConnection) {
    sqlx::query(
        "INSERT INTO checkouts (
             id, guest_secret_hash, owner_expires_at, request_fingerprint,
             convention_id, provider, provider_account_id, provider_environment,
             evidence_hash, required_tag_hashes, payment_state, completion_state,
             order_id, created_at, updated_at
         )
         SELECT
             gen_random_uuid(), decode(repeat('01', 32), 'hex'),
             TIMESTAMPTZ '2030-01-01 00:00:00+00', decode(repeat('02', 32), 'hex'),
             gen_random_uuid(), 'stripe', 'acct_migration_fixture', 'false',
             decode(repeat('03', 32), 'hex'), ARRAY[decode(repeat('04', 32), 'hex')],
             state.payment, state.completion, state.order_id,
             TIMESTAMPTZ '2000-01-01 00:00:00+00', TIMESTAMPTZ '2000-01-01 00:00:00+00'
         FROM (VALUES
             ('processing', 'not_started', NULL),
             ('paid', 'completed', 'gid://shopify/Order/migration-fixture'),
             ('unpaid', 'not_started', NULL)
         ) AS state(payment, completion, order_id)",
    )
    .execute(connection)
    .await
    .unwrap();
}

async fn assert_legacy_checkouts(connection: &mut PgConnection) {
    let unchanged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM checkouts
         WHERE guest_secret_hash = decode(repeat('01', 32), 'hex')
           AND request_fingerprint = decode(repeat('02', 32), 'hex')
           AND evidence_hash = decode(repeat('03', 32), 'hex')
           AND required_tag_hashes = ARRAY[decode(repeat('04', 32), 'hex')]
           AND provider_account_id = 'acct_migration_fixture'
           AND completion_version = 1
           AND correlation_version = 1
           AND convention_slug IS NULL
           AND payment_recording_state = 'not_started'
           AND recovery_failures = 0
           AND recovery_code IS NULL
           AND NOT recovery_manual
           AND created_at = TIMESTAMPTZ '2000-01-01 00:00:00+00'
           AND updated_at = created_at
           AND CASE
               WHEN payment_state = 'processing' AND completion_state = 'not_started'
                   THEN next_recovery_at = created_at + INTERVAL '60 seconds'
               WHEN payment_state = 'paid' AND completion_state = 'completed'
                   THEN next_recovery_at IS NULL
                       AND order_id = 'gid://shopify/Order/migration-fixture'
               WHEN payment_state = 'unpaid' AND completion_state = 'not_started'
                   THEN next_recovery_at IS NULL
               ELSE FALSE
           END",
    )
    .fetch_one(connection)
    .await
    .unwrap();
    assert_eq!(unchanged, 3);
}

async fn assert_checkout_constraints(connection: &mut PgConnection) {
    for assignment in [
        "recovery_failures = -1",
        "completion_version = 0",
        "completion_version = 2",
        "correlation_version = 0",
        "correlation_version = 2",
        "payment_recording_state = 'invalid'",
    ] {
        let mut savepoint = connection.begin().await.unwrap();
        let error = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE checkouts\nSET {assignment}\nWHERE payment_state = 'processing'"
        )))
        .execute(&mut *savepoint)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514"),
            "{assignment}: {error}"
        );
        savepoint.rollback().await.unwrap();
    }
    sqlx::query(
        "UPDATE checkouts
         SET completion_version = 2,
             correlation_version = 2,
             convention_slug = 'migration-fixture',
             payment_recording_state = 'recorded'
         WHERE payment_state = 'processing'",
    )
    .execute(connection)
    .await
    .unwrap();
}
