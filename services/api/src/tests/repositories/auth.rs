use super::*;
#[tokio::test]
#[ignore = "requires the configured Docker PostgreSQL; run just test"]
async fn postgres_cleanup_is_bounded_and_preserves_live_rows() {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("database");
    sqlx::raw_sql("CREATE TEMP TABLE refresh_tokens (id UUID PRIMARY KEY, expires_at TIMESTAMPTZ NOT NULL, revoked_at TIMESTAMPTZ); INSERT INTO refresh_tokens SELECT gen_random_uuid(),NOW()-interval '1 second',NULL FROM generate_series(1,501); INSERT INTO refresh_tokens VALUES (gen_random_uuid(),NOW()+interval '1 day',NOW()),(gen_random_uuid(),NOW()+interval '1 day',NULL);").execute(&pool).await.expect("owned temporary fixtures");
    assert_eq!(cleanup_expired(&pool).await.expect("first batch"), 500);
    assert_eq!(cleanup_expired(&pool).await.expect("second batch"), 2);
    assert_eq!(cleanup_expired(&pool).await.expect("idempotent"), 0);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE expires_at>NOW() AND revoked_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("live count");
    assert_eq!(count, 1);
    pool.close().await;
}
