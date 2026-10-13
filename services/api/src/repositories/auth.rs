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
