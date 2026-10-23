use crate::{error::AppError, models::checkout::Checkout};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<Checkout>, AppError> {
    Ok(sqlx::query_as("SELECT *\nFROM checkouts\nWHERE id = $1")
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
        "SELECT *\nFROM checkouts\nWHERE provider = $1 AND provider_resource_id = $2",
    )
    .bind(provider)
    .bind(resource)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn rate_limit(
    c: &mut PgConnection,
    source: &[u8],
    seconds: i64,
    maximum: i64,
) -> Result<bool, AppError> {
    let attempts: i32 = sqlx::query_scalar(
        "INSERT INTO checkout_rate_limits (source_hash, window_started_at, attempts)
         VALUES ($1, CURRENT_TIMESTAMP, 1)
         ON CONFLICT (source_hash) DO UPDATE
         SET attempts = CASE
                 WHEN checkout_rate_limits.window_started_at <= CURRENT_TIMESTAMP - make_interval(secs => $2::bigint::double precision)
                 THEN 1 ELSE checkout_rate_limits.attempts + 1 END,
             window_started_at = CASE
                 WHEN checkout_rate_limits.window_started_at <= CURRENT_TIMESTAMP - make_interval(secs => $2::bigint::double precision)
                 THEN CURRENT_TIMESTAMP ELSE checkout_rate_limits.window_started_at END
         RETURNING attempts")
        .bind(source).bind(seconds).fetch_one(c).await?;
    Ok(i64::from(attempts) <= maximum)
}

pub(crate) async fn insert(c: &mut PgConnection, checkout: &Checkout) -> Result<bool, AppError> {
    let inserted = sqlx::query(
        "INSERT INTO checkouts (
             id, operation_id, guest_secret_hash, owner_expires_at, request_fingerprint,
             provider, snapshot, provider_context, create_request_id, capture_request_id
         )
         VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (operation_id) DO NOTHING",
    )
    .bind(checkout.id)
    .bind(&checkout.guest_secret_hash)
    .bind(checkout.owner_expires_at)
    .bind(&checkout.request_fingerprint)
    .bind(&checkout.provider)
    .bind(&checkout.snapshot)
    .bind(&checkout.provider_context)
    .bind(&checkout.create_request_id)
    .bind(&checkout.capture_request_id)
    .execute(&mut *c)
    .await?
    .rows_affected()
        == 1;
    if inserted {
        for item in &checkout.snapshot.items {
            sqlx::query(
                "INSERT INTO checkout_items (
                     checkout_id, variant_id, shopify_product_id, shopify_variant_id,
                     product_title, variant_title, sku, quantity, unit_price_minor, total_minor
                 )
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(checkout.id)
            .bind(item.variant_id)
            .bind(&item.shopify_product_id)
            .bind(&item.shopify_variant_id)
            .bind(&item.product_title)
            .bind(&item.variant_title)
            .bind(&item.sku)
            .bind(i32::try_from(item.quantity).map_err(|_| {
                AppError::new(
                    axum::http::StatusCode::CONFLICT,
                    "Invalid checkout quantity",
                )
            })?)
            .bind(item.unit_price_minor)
            .bind(item.total_minor)
            .execute(&mut *c)
            .await?;
        }
    }
    Ok(inserted)
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
             payment_reference = $8,
             order_id = $9,
             order_name = $10,
             redirect_url = $11,
             reservation_expires_at = $12,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = $1 AND claim_token = $2 AND claim_expires_at > CURRENT_TIMESTAMP",
    )
    .bind(checkout.id)
    .bind(token)
    .bind(checkout.payment_state)
    .bind(checkout.draft_state)
    .bind(checkout.completion_state)
    .bind(&checkout.draft_id)
    .bind(&checkout.provider_resource_id)
    .bind(&checkout.payment_reference)
    .bind(&checkout.order_id)
    .bind(&checkout.order_name)
    .bind(&checkout.redirect_url)
    .bind(checkout.reservation_expires_at)
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
         WHERE id = $1 AND claim_token = $2",
    )
    .bind(id)
    .bind(token)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn event(
    c: &mut PgConnection,
    checkout: &Checkout,
    event_id: &str,
) -> Result<bool, AppError> {
    let row: (Uuid, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "INSERT INTO checkout_events (provider, account_id, environment, event_id, checkout_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider, account_id, environment, event_id) DO UPDATE
         SET event_id = EXCLUDED.event_id
         RETURNING checkout_id, processed_at",
    )
    .bind(&checkout.provider)
    .bind(&checkout.provider_context.account_id)
    .bind(&checkout.provider_context.environment)
    .bind(event_id)
    .bind(checkout.id)
    .fetch_one(c)
    .await?;
    if row.0 != checkout.id {
        return Err(AppError::new(
            axum::http::StatusCode::CONFLICT,
            "Webhook association conflicts",
        ));
    }
    Ok(row.1.is_some())
}

pub(crate) async fn finish_event(
    pool: &PgPool,
    checkout: &Checkout,
    event_id: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE checkout_events
         SET processed_at = CURRENT_TIMESTAMP
         WHERE provider = $1 AND account_id = $2 AND environment = $3 AND event_id = $4 AND checkout_id = $5")
        .bind(&checkout.provider).bind(&checkout.provider_context.account_id)
        .bind(&checkout.provider_context.environment).bind(event_id).bind(checkout.id)
        .execute(pool).await?;
    Ok(())
}
