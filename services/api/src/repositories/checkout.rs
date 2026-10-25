use crate::{
    error::AppError,
    models::checkout::{Checkout, RecoveryCode},
};
use chrono::{DateTime, Utc};
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
             evidence_hash, required_tag_hashes, success_url, cancel_url, next_recovery_at,
             completion_version, convention_slug, payment_recording_state, correlation_version, created_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 $17::TIMESTAMPTZ + INTERVAL '60 seconds', $13, $14, $15, $16, $17)
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
    .bind(checkout.completion_version)
    .bind(&checkout.convention_slug)
    .bind(checkout.payment_recording_state)
    .bind(checkout.correlation_version)
    .bind(checkout.created_at)
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
        "WITH locked AS MATERIALIZED (
             SELECT id, claim_token, claim_expires_at
             FROM checkouts
             WHERE id = $1
             FOR UPDATE
         )
         UPDATE checkouts AS c
         SET claim_token = $2,
             claim_expires_at = clock_timestamp() + INTERVAL '10 minutes'
         FROM locked
         WHERE c.id = locked.id
           AND (locked.claim_token IS NULL OR locked.claim_expires_at <= clock_timestamp())
         RETURNING c.*",
    )
    .bind(id)
    .bind(token)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn save(pool: &PgPool, checkout: &Checkout, token: Uuid) -> Result<(), AppError> {
    let affected = sqlx::query(
        "WITH locked AS MATERIALIZED (
             SELECT id, claim_token, claim_expires_at
             FROM checkouts
             WHERE id = $1
             FOR UPDATE
         )
         UPDATE checkouts AS c
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
             payment_recording_state = $13,
             next_recovery_at = CASE WHEN $5 = 'completed' OR $3 = 'unpaid'
                 THEN NULL
                 WHEN c.payment_state = 'unpaid' THEN clock_timestamp() + INTERVAL '60 seconds'
                 ELSE c.next_recovery_at END,
             recovery_failures = CASE WHEN $5 = 'completed' OR $3 = 'unpaid' OR c.payment_state = 'unpaid'
                 THEN 0 ELSE c.recovery_failures END,
             recovery_code = CASE WHEN $5 = 'completed' OR $3 = 'unpaid' OR c.payment_state = 'unpaid'
                 THEN NULL ELSE c.recovery_code END,
             recovery_manual = CASE WHEN $5 = 'completed' OR $3 = 'unpaid' OR c.payment_state = 'unpaid'
                 THEN FALSE ELSE c.recovery_manual END,
             updated_at = clock_timestamp()
         FROM locked
         WHERE c.id = locked.id
           AND locked.claim_token = $2
           AND locked.claim_expires_at > clock_timestamp()",
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
    .bind(checkout.payment_recording_state)
    .execute(pool)
    .await?
    .rows_affected();
    require_claim(affected)
}

pub(crate) async fn release(pool: &PgPool, id: Uuid, token: Uuid) -> Result<(), AppError> {
    let affected = sqlx::query(
        "WITH locked AS MATERIALIZED (
             SELECT id, claim_token, claim_expires_at
             FROM checkouts
             WHERE id = $1
             FOR UPDATE
         )
         UPDATE checkouts AS c
         SET claim_token = NULL,
             claim_expires_at = NULL
         FROM locked
         WHERE c.id = locked.id
           AND locked.claim_token = $2
           AND locked.claim_expires_at > clock_timestamp()",
    )
    .bind(id)
    .bind(token)
    .execute(pool)
    .await?
    .rows_affected();
    require_claim(affected)
}

pub(crate) enum DueClaim {
    Ready {
        checkout: Box<Checkout>,
        failures: u32,
    },
    Abandoned {
        id: Uuid,
        failures: u32,
    },
    Stopped {
        id: Uuid,
    },
}

pub(crate) async fn claim_due(
    pool: &PgPool,
    token: Uuid,
    lease_secs: u32,
    max_failures: u32,
) -> Result<Option<DueClaim>, AppError> {
    let row: Option<DueRow> = sqlx::query_as(
        "WITH candidate AS MATERIALIZED (
             SELECT id,
                    claim_token IS NOT NULL AS abandoned,
                    LEAST(recovery_failures::BIGINT + CASE WHEN claim_token IS NOT NULL THEN 1 ELSE 0 END,
                          2147483647)::INTEGER AS failures
             FROM checkouts
             WHERE next_recovery_at <= statement_timestamp()
               AND NOT recovery_manual
               AND completion_state <> 'completed'
               AND payment_state <> 'unpaid'
               AND (claim_token IS NULL OR claim_expires_at <= clock_timestamp())
             ORDER BY next_recovery_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT 1
         )
         UPDATE checkouts AS c
         SET recovery_failures = candidate.failures,
             recovery_manual = candidate.failures >= $3::BIGINT,
             next_recovery_at = CASE WHEN candidate.failures >= $3::BIGINT
                 THEN NULL ELSE c.next_recovery_at END,
             recovery_code = CASE WHEN candidate.failures >= $3::BIGINT
                 THEN 'attempts_exhausted' ELSE c.recovery_code END,
             claim_token = CASE WHEN candidate.failures >= $3::BIGINT THEN NULL ELSE $1 END,
             claim_expires_at = CASE WHEN candidate.failures >= $3::BIGINT THEN NULL
                 ELSE clock_timestamp() + make_interval(secs => $2::DOUBLE PRECISION) END,
             updated_at = clock_timestamp()
         FROM candidate
         WHERE c.id = candidate.id
         RETURNING c.*, candidate.abandoned",
    )
    .bind(token)
    .bind(f64::from(lease_secs))
    .bind(i64::from(max_failures))
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let failures = u32::try_from(row.recovery_failures).map_err(anyhow::Error::new)?;
    Ok(Some(if row.recovery_manual {
        DueClaim::Stopped {
            id: row.checkout.id,
        }
    } else if row.abandoned {
        DueClaim::Abandoned {
            id: row.checkout.id,
            failures,
        }
    } else {
        DueClaim::Ready {
            checkout: Box::new(row.checkout),
            failures,
        }
    }))
}

pub(crate) async fn renew(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    lease_secs: u32,
) -> Result<(), AppError> {
    let affected = sqlx::query(
        "WITH locked AS MATERIALIZED (
             SELECT id, claim_token, claim_expires_at
             FROM checkouts
             WHERE id = $1
             FOR UPDATE
         )
         UPDATE checkouts AS c
         SET claim_expires_at = clock_timestamp() + make_interval(secs => $3::DOUBLE PRECISION)
         FROM locked
         WHERE c.id = locked.id
           AND locked.claim_token = $2
           AND locked.claim_expires_at > clock_timestamp()",
    )
    .bind(id)
    .bind(token)
    .bind(f64::from(lease_secs))
    .execute(pool)
    .await?
    .rows_affected();
    require_claim(affected)
}

pub(crate) async fn finish_recovery(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    failures: u32,
    next_due: Option<DateTime<Utc>>,
    manual: bool,
    code: RecoveryCode,
) -> Result<(), AppError> {
    let failures = i32::try_from(failures).map_err(anyhow::Error::new)?;
    let affected = sqlx::query(
        "WITH locked AS MATERIALIZED (
             SELECT id, claim_token, claim_expires_at
             FROM checkouts
             WHERE id = $1
             FOR UPDATE
         )
         UPDATE checkouts AS c
         SET recovery_failures = CASE WHEN c.completion_state = 'completed' OR c.payment_state = 'unpaid'
                 THEN 0 ELSE $3 END,
             next_recovery_at = CASE WHEN c.completion_state = 'completed' OR c.payment_state = 'unpaid'
                 THEN NULL ELSE $4 END,
             recovery_manual = CASE WHEN c.completion_state = 'completed' OR c.payment_state = 'unpaid'
                 THEN FALSE ELSE $5 END,
             recovery_code = CASE WHEN c.completion_state = 'completed' OR c.payment_state = 'unpaid'
                 THEN NULL ELSE $6 END,
             claim_token = NULL,
             claim_expires_at = NULL,
             updated_at = clock_timestamp()
         FROM locked
         WHERE c.id = locked.id
           AND locked.claim_token = $2
           AND locked.claim_expires_at > clock_timestamp()",
    )
    .bind(id)
    .bind(token)
    .bind(failures)
    .bind(next_due)
    .bind(manual)
    .bind(code)
    .execute(pool)
    .await?
    .rows_affected();
    require_claim(affected)
}

#[derive(sqlx::FromRow)]
struct DueRow {
    #[sqlx(flatten)]
    checkout: Checkout,
    recovery_failures: i32,
    recovery_manual: bool,
    abandoned: bool,
}

fn require_claim(affected: u64) -> Result<(), AppError> {
    if affected != 1 {
        return Err(AppError::new(
            axum::http::StatusCode::CONFLICT,
            "Checkout work claim expired",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/repositories/checkout.rs"]
mod tests;
