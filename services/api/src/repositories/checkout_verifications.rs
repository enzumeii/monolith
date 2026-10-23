use crate::error::AppError;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) async fn claim(pool: &PgPool, id: Uuid, token: Uuid) -> Result<bool, AppError> {
    let count=sqlx::query("UPDATE checkout_completions SET next_verify_at=CURRENT_TIMESTAMP+INTERVAL '5 seconds',lease_token=$2,lease_until=CURRENT_TIMESTAMP+INTERVAL '120 seconds',updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND guest_token_expires_at>CURRENT_TIMESTAMP AND guest_token_revoked_at IS NULL AND (next_verify_at IS NULL OR next_verify_at<=CURRENT_TIMESTAMP) AND (lease_until IS NULL OR lease_until<=CURRENT_TIMESTAMP) AND completion_state NOT IN ('COMPLETE','OPERATOR')")
        .bind(id).bind(token).execute(pool).await?.rows_affected();
    Ok(count == 1)
}

pub(crate) async fn release(pool: &PgPool, id: Uuid, token: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE checkout_completions SET lease_token=NULL,lease_until=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND lease_token=$2").bind(id).bind(token).execute(pool).await?;
    Ok(())
}
