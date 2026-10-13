use crate::error::AppError;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};

pub(crate) async fn create_refresh_token(
    connection: &mut PgConnection,
    admin_id: i64,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO refresh_tokens (admin_id, token_hash, expires_at) VALUES ($1,$2,$3)",
        admin_id,
        token_hash,
        expires_at
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
) -> Result<bool, AppError> {
    let result = sqlx::query!("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND admin_id = $2 AND revoked_at IS NULL AND expires_at > NOW()", hash, admin_id).execute(connection).await?;
    Ok(result.rows_affected() == 1)
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
