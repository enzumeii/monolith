use crate::error::AppError;
use crate::models::{DbAdminRole, TokenRefreshRecord};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[tracing::instrument(skip(pool))]
pub async fn create_refresh_token(
    pool: &PgPool,
    admin_id: i64,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
                INSERT INTO refresh_tokens (admin_id,token_hash,expires_at)
                VALUES ($1, $2, $3)
            "#,
        admin_id,
        token_hash,
        expires_at
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[tracing::instrument(skip(pool))]
pub async fn consume_refresh_token(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<TokenRefreshRecord>, AppError> {
    let record = sqlx::query_as!(
        TokenRefreshRecord,
        r#"
            UPDATE refresh_tokens AS rt
            SET revoked_at = NOW()
            FROM admin AS a
            WHERE rt.admin_id = a.id
              AND rt.token_hash = $1
              AND rt.revoked_at IS NULL
              AND rt.expires_at > NOW()
            RETURNING rt.id AS token_id, rt.admin_id, a.username,
                      a.role AS "role: DbAdminRole", rt.expires_at, rt.revoked_at
            "#,
        token_hash
    )
    .fetch_optional(pool)
    .await?;

    Ok(record)
}

#[tracing::instrument(skip(pool))]
pub async fn delete_all_refresh_tokens(pool: &PgPool, admin_id: i64) -> Result<(), AppError> {
    sqlx::query!(
        r#"
            DELETE FROM refresh_tokens
            WHERE admin_id = $1
            "#,
        admin_id
    )
    .execute(pool)
    .await?;

    Ok(())
}
