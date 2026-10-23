use crate::{error::AppError, models::checkout::Checkout};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<Checkout>, AppError> {
    Ok(sqlx::query_as(
        "SELECT *
         FROM checkouts
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn by_resource(
    pool: &PgPool,
    provider: &str,
    resource: &str,
) -> Result<Option<Checkout>, AppError> {
    Ok(sqlx::query_as(
        "SELECT *
         FROM checkouts
         WHERE provider = $1
           AND provider_resource_id = $2",
    )
    .bind(provider)
    .bind(resource)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn insert(c: &mut PgConnection, checkout: &Checkout) -> Result<bool, AppError> {
    Ok(sqlx::query(
        "INSERT INTO checkouts (
             id, guest_secret_hash, owner_expires_at, request_fingerprint,
             convention_id, provider, provider_account_id, provider_environment,
             evidence_hash, required_tag_hashes, success_url, cancel_url
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(checkout.id)
    .bind(&checkout.guest_secret_hash)
    .bind(checkout.owner_expires_at)
    .bind(&checkout.request_fingerprint)
    .bind(checkout.convention_id)
    .bind(&checkout.provider)
    .bind(&checkout.provider_account_id)
    .bind(&checkout.provider_environment)
    .bind(&checkout.evidence_hash)
    .bind(&checkout.required_tag_hashes)
    .bind(&checkout.success_url)
    .bind(&checkout.cancel_url)
    .execute(c)
    .await?
    .rows_affected()
        == 1)
}

pub(crate) async fn claim(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
) -> Result<Option<Checkout>, AppError> {
    Ok(sqlx::query_as(
        "UPDATE checkouts
         SET claim_token = $2,
             claim_expires_at = CURRENT_TIMESTAMP + INTERVAL '10 minutes'
         WHERE id = $1
           AND (claim_token IS NULL OR claim_expires_at <= CURRENT_TIMESTAMP)
         RETURNING *",
    )
    .bind(id)
    .bind(token)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn save(pool: &PgPool, checkout: &Checkout, token: Uuid) -> Result<(), AppError> {
    let affected = sqlx::query(
        "UPDATE checkouts
         SET payment_state = $3,
             draft_state = $4,
             completion_state = $5,
             draft_id = $6,
             provider_resource_id = $7,
             order_id = $8,
             reservation_expires_at = $9,
             success_url = $10,
             cancel_url = $11,
             gateway_request_fingerprint = $12,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = $1
           AND claim_token = $2
           AND claim_expires_at > CURRENT_TIMESTAMP",
    )
    .bind(checkout.id)
    .bind(token)
    .bind(checkout.payment_state)
    .bind(checkout.draft_state)
    .bind(checkout.completion_state)
    .bind(&checkout.draft_id)
    .bind(&checkout.provider_resource_id)
    .bind(&checkout.order_id)
    .bind(checkout.reservation_expires_at)
    .bind(&checkout.success_url)
    .bind(&checkout.cancel_url)
    .bind(&checkout.gateway_request_fingerprint)
    .execute(pool)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(AppError::new(
            axum::http::StatusCode::CONFLICT,
            "Checkout work claim expired",
        ));
    }
    Ok(())
}

pub(crate) async fn release(pool: &PgPool, id: Uuid, token: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE checkouts
         SET claim_token = NULL, claim_expires_at = NULL
         WHERE id = $1
           AND claim_token = $2",
    )
    .bind(id)
    .bind(token)
    .execute(pool)
    .await?;
    Ok(())
}
