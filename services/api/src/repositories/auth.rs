use crate::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

pub(crate) async fn create_refresh_token(
    connection: &mut PgConnection,
    admin_id: i64,
    token_hash: &str,
    expires_at: DateTime<Utc>,
    family_id: Option<uuid::Uuid>,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO refresh_tokens (admin_id, token_hash, expires_at, family_id) VALUES ($1,$2,$3,COALESCE($4,gen_random_uuid()))",
        admin_id,
        token_hash,
        expires_at,
        family_id
    )
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn refresh_owner(pool: &PgPool, hash: &str) -> Result<Option<i64>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT admin_id FROM refresh_tokens WHERE token_hash = $1",
        hash
    )
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn consume_refresh_token(
    connection: &mut PgConnection,
    hash: &str,
    admin_id: i64,
) -> Result<Option<uuid::Uuid>, AppError> {
    Ok(sqlx::query_scalar!("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND admin_id = $2 AND revoked_at IS NULL AND expires_at > NOW() RETURNING family_id",hash,admin_id).fetch_optional(connection).await?)
}

pub(crate) async fn delete_family(
    connection: &mut PgConnection,
    id: i64,
    hash: &str,
) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM refresh_tokens WHERE admin_id = $1 AND family_id = (SELECT family_id FROM refresh_tokens WHERE admin_id = $1 AND token_hash = $2)",id,hash).execute(connection).await?;
    Ok(())
}

pub(crate) async fn delete_refresh_tokens(
    connection: &mut PgConnection,
    admin_id: i64,
) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM refresh_tokens WHERE admin_id = $1", admin_id)
        .execute(connection)
        .await?;
    Ok(())
}

pub(crate) async fn revoke_account(connection: &mut PgConnection, id: i64) -> Result<(), AppError> {
    delete_refresh_tokens(connection, id).await
}

pub async fn cleanup_expired(pool: &sqlx::PgPool) -> Result<u64, AppError> {
    let result=sqlx::query("DELETE FROM refresh_tokens WHERE id IN (SELECT id FROM refresh_tokens WHERE expires_at < NOW() OR revoked_at IS NOT NULL ORDER BY expires_at LIMIT 500 FOR UPDATE SKIP LOCKED)").execute(pool).await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires the configured Docker PostgreSQL; run just test"]
    async fn postgres_cleanup_is_bounded_and_preserves_live_rows() {
        let url = std::env::var("E2E_DATABASE_URL").expect("Compose database URL");
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
}
