use crate::{
    error::AppError,
    models::paypal_payments::{
        CaptureEvidence, NewPaypalPayment, NewPaypalPaymentEvent, PaymentOperation, PaypalPayment,
        PaypalPaymentEvent,
    },
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn create_or_get(
    pool: &PgPool,
    input: &NewPaypalPayment,
) -> Result<(PaypalPayment, bool), AppError> {
    let inserted = sqlx::query_as::<_, PaypalPayment>(
        "INSERT INTO paypal_payments(id,idempotency_key,capability_digest,reference,amount,currency,minor_units,merchant_id,environment,api_origin,create_request_id,capture_request_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(idempotency_key) DO NOTHING RETURNING *"
    ).bind(input.id).bind(input.idempotency_key).bind(input.capability_digest.as_slice())
    .bind(&input.reference).bind(&input.amount).bind(&input.currency).bind(input.minor_units)
    .bind(&input.merchant_id).bind(&input.environment).bind(&input.api_origin)
    .bind(input.create_request_id).bind(input.capture_request_id).fetch_optional(pool).await?;
    if let Some(payment) = inserted {
        return Ok((payment, true));
    }
    let payment = sqlx::query_as("SELECT * FROM paypal_payments WHERE idempotency_key=$1")
        .bind(input.idempotency_key)
        .fetch_one(pool)
        .await?;
    Ok((payment, false))
}

pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<PaypalPayment>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM paypal_payments WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub(crate) async fn find_by_order(
    pool: &PgPool,
    merchant: &str,
    environment: &str,
    order_id: &str,
) -> Result<Option<PaypalPayment>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM paypal_payments WHERE merchant_id=$1 AND environment=$2 AND paypal_order_id=$3")
        .bind(merchant).bind(environment).bind(order_id).fetch_optional(pool).await?)
}

pub(crate) async fn lock(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<PaypalPayment>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM paypal_payments WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(connection)
            .await?,
    )
}

pub(crate) async fn claim_operation(
    pool: &PgPool,
    id: Uuid,
    operation: PaymentOperation,
    token: Uuid,
    until: DateTime<Utc>,
) -> Result<Option<PaypalPayment>, AppError> {
    Ok(sqlx::query_as("UPDATE paypal_payments SET operation=$2,lease_token=$3,lease_until=$4,create_started_at=CASE WHEN $2='CREATE' THEN COALESCE(create_started_at,clock_timestamp()) ELSE create_started_at END,capture_started_at=CASE WHEN $2='CAPTURE' THEN COALESCE(capture_started_at,clock_timestamp()) ELSE capture_started_at END,updated_at=clock_timestamp() WHERE id=$1 AND (lease_until IS NULL OR lease_until<=clock_timestamp()) AND $4>clock_timestamp() RETURNING *")
        .bind(id).bind(operation.as_str()).bind(token).bind(until).fetch_optional(pool).await?)
}

pub(crate) async fn save_order(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    order_id: &str,
    approval_url: Option<&str>,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE paypal_payments SET paypal_order_id=$3,approval_url=$4,status='AWAITING_APPROVAL',operation=NULL,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE id=$1 AND lease_token=$2 AND lease_until>clock_timestamp() AND verified_at IS NULL AND (paypal_order_id IS NULL OR paypal_order_id=$3)")
        .bind(id).bind(token).bind(order_id).bind(approval_url).execute(pool).await?.rows_affected()==1)
}

pub(crate) async fn finish_operation(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    status: &str,
    exception: Option<&str>,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE paypal_payments SET status=CASE WHEN verified_at IS NOT NULL THEN status ELSE $3 END,exception=COALESCE($4,exception),operation=NULL,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE id=$1 AND lease_token=$2 AND lease_until>clock_timestamp()")
        .bind(id).bind(token).bind(status).bind(exception).execute(pool).await?.rows_affected()==1)
}

pub(crate) async fn captures(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Vec<CaptureEvidence>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM paypal_payment_captures WHERE payment_id=$1 ORDER BY verified_at,capture_id",
    )
    .bind(id)
    .fetch_all(connection)
    .await?)
}

pub(crate) async fn save_capture(
    connection: &mut PgConnection,
    capture: &CaptureEvidence,
) -> Result<bool, AppError> {
    Ok(sqlx::query("INSERT INTO paypal_payment_captures(capture_id,payment_id,merchant_id,environment,paypal_order_id,amount,currency,minor_units,provider_status,verified_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10 WHERE EXISTS(SELECT 1 FROM paypal_payments WHERE id=$2 AND merchant_id=$3 AND environment=$4 AND paypal_order_id=$5 AND amount=$6 AND currency=$7 AND minor_units=$8) ON CONFLICT(merchant_id,environment,capture_id) DO UPDATE SET capture_id=EXCLUDED.capture_id WHERE paypal_payment_captures.payment_id=EXCLUDED.payment_id AND paypal_payment_captures.paypal_order_id=EXCLUDED.paypal_order_id AND paypal_payment_captures.amount=EXCLUDED.amount AND paypal_payment_captures.currency=EXCLUDED.currency AND paypal_payment_captures.minor_units=EXCLUDED.minor_units AND paypal_payment_captures.provider_status=EXCLUDED.provider_status")
        .bind(&capture.capture_id).bind(capture.payment_id).bind(&capture.merchant_id).bind(&capture.environment).bind(&capture.paypal_order_id).bind(&capture.amount).bind(&capture.currency).bind(capture.minor_units).bind(&capture.provider_status).bind(capture.verified_at).execute(connection).await?.rows_affected()==1)
}

pub(crate) async fn lock_reference(
    connection: &mut PgConnection,
    payment: &PaypalPayment,
) -> Result<(), AppError> {
    // Acquire before payment row locks so duplicate flagging cannot deadlock another completion.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(concat_ws(':',$1::text,$2::text,$3::text),0))")
        .bind(&payment.merchant_id).bind(&payment.environment).bind(&payment.reference)
        .execute(connection).await?;
    Ok(())
}

pub(crate) async fn complete(
    connection: &mut PgConnection,
    id: Uuid,
    token: Option<Uuid>,
    capture_id: &str,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE paypal_payments SET status='COMPLETED',verified_capture_id=$3,verified_at=COALESCE(verified_at,clock_timestamp()),operation=NULL,lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE id=$1 AND ($2::uuid IS NULL OR (lease_token=$2 AND lease_until>clock_timestamp())) AND (verified_capture_id IS NULL OR verified_capture_id=$3) AND EXISTS(SELECT 1 FROM paypal_payment_captures c WHERE c.payment_id=$1 AND c.capture_id=$3 AND c.provider_status='COMPLETED')")
        .bind(id).bind(token).bind(capture_id).execute(connection).await?.rows_affected()==1)
}

pub(crate) async fn duplicate_successful_reference(
    connection: &mut PgConnection,
    payment: &PaypalPayment,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar("SELECT id FROM paypal_payments WHERE merchant_id=$1 AND environment=$2 AND reference=$3 AND id<>$4 AND verified_at IS NOT NULL ORDER BY id")
        .bind(&payment.merchant_id).bind(&payment.environment).bind(&payment.reference).bind(payment.id).fetch_all(connection).await?)
}

pub(crate) async fn flag_exception(
    connection: &mut PgConnection,
    id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE paypal_payments SET exception=$2,status=CASE WHEN verified_at IS NOT NULL THEN status ELSE 'EXCEPTION' END,updated_at=clock_timestamp() WHERE id=$1")
        .bind(id).bind(reason).execute(connection).await?;
    Ok(())
}

pub(crate) async fn record_event(
    pool: &PgPool,
    event: &NewPaypalPaymentEvent,
) -> Result<(PaypalPaymentEvent, bool), AppError> {
    let inserted = sqlx::query_as("INSERT INTO paypal_payment_events(merchant_id,environment,event_id,payload_digest,event_type,paypal_order_id,capture_id,verified_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(merchant_id,environment,event_id) DO NOTHING RETURNING *")
        .bind(&event.merchant_id).bind(&event.environment).bind(&event.event_id).bind(event.payload_digest.as_slice()).bind(&event.event_type).bind(&event.paypal_order_id).bind(&event.capture_id).bind(event.verified_at).fetch_optional(pool).await?;
    if let Some(event) = inserted {
        return Ok((event, true));
    }
    let saved = sqlx::query_as("SELECT * FROM paypal_payment_events WHERE merchant_id=$1 AND environment=$2 AND event_id=$3")
        .bind(&event.merchant_id).bind(&event.environment).bind(&event.event_id).fetch_one(pool).await?;
    Ok((saved, false))
}

pub(crate) async fn lock_event(
    connection: &mut PgConnection,
    merchant: &str,
    environment: &str,
    event_id: &str,
) -> Result<PaypalPaymentEvent, AppError> {
    Ok(sqlx::query_as("SELECT * FROM paypal_payment_events WHERE merchant_id=$1 AND environment=$2 AND event_id=$3 FOR UPDATE")
        .bind(merchant).bind(environment).bind(event_id).fetch_one(connection).await?)
}

pub(crate) async fn finish_event(
    connection: &mut PgConnection,
    merchant: &str,
    environment: &str,
    event_id: &str,
    payment_id: Option<Uuid>,
    ignored: bool,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE paypal_payment_events SET state=$5,payment_id=$4,processed_at=clock_timestamp(),updated_at=clock_timestamp() WHERE merchant_id=$1 AND environment=$2 AND event_id=$3 AND state='PENDING'")
        .bind(merchant).bind(environment).bind(event_id).bind(payment_id).bind(if ignored {"IGNORED"} else {"PROCESSED"}).execute(connection).await?.rows_affected()==1)
}

#[cfg(test)]
#[path = "../tests/repositories/paypal_payments.rs"]
mod tests;
