use crate::{
    error::AppError,
    models::{
        checkout_completion::{
            AliasKind, CompletionRow, CompletionState, PaymentAlias, PaymentAttempt,
            PaymentEnvironment, PaymentFact, PaymentState, Provider, Snapshot, VerifiedPayment,
            conflict,
        },
        shopify::ShopifyId,
    },
};
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

#[allow(
    dead_code,
    reason = "ENZ-42 provisions the persisted checkout contract on the later dependency rebase"
)]
pub(crate) async fn insert(
    connection: &mut PgConnection,
    id: Uuid,
    snapshot: &Snapshot,
    attempt: &PaymentAttempt,
    token_hash: &[u8],
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO checkout_completions(id,draft_id,snapshot,attempt,guest_token_hash,guest_token_expires_at) VALUES($1,$2,$3::jsonb,$4::jsonb,$5,$6)")
        .bind(id).bind(snapshot.draft_id.as_ref()).bind(encode(snapshot)?).bind(encode(attempt)?).bind(token_hash).bind(expires_at).execute(connection).await?;
    Ok(())
}
pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<CompletionRow>, AppError> {
    sqlx::query_as::<_,StoredRow>("SELECT id,snapshot::text,attempt::text,guest_token_hash,guest_token_expires_at,guest_token_revoked_at,next_verify_at,payment_state,completion_state,trusted_payment::text,order_id,completion_intent,lease_token,lease_until FROM checkout_completions WHERE id=$1").bind(id).fetch_optional(pool).await?.map(StoredRow::decode).transpose()
}
pub(crate) async fn claim(
    connection: &mut PgConnection,
    id: Uuid,
    token: Uuid,
    until: DateTime<Utc>,
) -> Result<Option<CompletionRow>, AppError> {
    sqlx::query_as::<_,StoredRow>("UPDATE checkout_completions SET lease_token=$2,lease_until=$3,updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND (lease_until IS NULL OR lease_until<=CURRENT_TIMESTAMP) RETURNING id,snapshot::text,attempt::text,guest_token_hash,guest_token_expires_at,guest_token_revoked_at,next_verify_at,payment_state,completion_state,trusted_payment::text,order_id,completion_intent,lease_token,lease_until")
        .bind(id).bind(token).bind(until).fetch_optional(connection).await?.map(StoredRow::decode).transpose()
}
pub(crate) async fn record_fact(
    connection: &mut PgConnection,
    id: Uuid,
    token: Uuid,
    fact: &PaymentFact,
    event_id: &str,
) -> Result<(), AppError> {
    let state = match fact {
        PaymentFact::Pending => PaymentState::Pending,
        PaymentFact::Unpaid => PaymentState::Unpaid,
        PaymentFact::Expired => PaymentState::Expired,
        PaymentFact::Paid { .. } => PaymentState::Paid,
    };
    let count=sqlx::query("UPDATE checkout_completions SET payment_state=$3,trusted_payment=CASE WHEN $3='PAID' THEN $4::jsonb ELSE trusted_payment END,last_event_id=$5,updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND lease_token=$2 AND lease_until>CURRENT_TIMESTAMP AND (payment_state<>'PAID' OR $3='PAID')")
        .bind(id).bind(token).bind(code(state)?).bind(encode(fact)?).bind(event_id).execute(connection).await?.rows_affected();
    owned(count)
}
pub(crate) async fn bind_aliases(
    connection: &mut PgConnection,
    id: Uuid,
    attempt: &PaymentAttempt,
    aliases: &[PaymentAlias],
) -> Result<(), AppError> {
    for alias in aliases {
        let result=sqlx::query("INSERT INTO checkout_payment_aliases(checkout_id,provider,account,environment,kind,value) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(provider,account,environment,kind,value) DO UPDATE SET value=EXCLUDED.value WHERE checkout_payment_aliases.checkout_id=EXCLUDED.checkout_id")
            .bind(id).bind(code(attempt.provider)?).bind(&attempt.account).bind(code(attempt.environment)?).bind(code(alias.kind)?).bind(&alias.value).execute(&mut *connection).await;
        match result {
            Ok(done) if done.rows_affected() == 1 => {}
            Ok(_) => {
                return Err(conflict(
                    "Payment reference already belongs to another checkout",
                ));
            }
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation) =>
            {
                return Err(
                    conflict("Payment reference conflicts with checkout history")
                        .with_source(error),
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn progress(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    state: CompletionState,
    order_id: Option<&ShopifyId>,
    intent: bool,
    last_error: Option<&str>,
    release: bool,
) -> Result<(), AppError> {
    let count=sqlx::query("UPDATE checkout_completions SET completion_state=$3,order_id=COALESCE(order_id,$4),completion_intent=completion_intent OR $5,last_error=$6,lease_token=CASE WHEN $7 THEN NULL ELSE lease_token END,lease_until=CASE WHEN $7 THEN NULL ELSE lease_until END,updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND lease_token=$2 AND lease_until>CURRENT_TIMESTAMP AND (order_id IS NULL OR $4::text IS NULL OR order_id=$4)")
        .bind(id).bind(token).bind(code(state)?).bind(order_id.map(AsRef::as_ref)).bind(intent).bind(last_error).bind(release).execute(pool).await?.rows_affected();
    owned(count)
}
pub(crate) async fn save_operator_payment(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    payment: &VerifiedPayment,
) -> Result<(), AppError> {
    let paid = matches!(payment.state, PaymentFact::Paid { .. });
    let count=sqlx::query("UPDATE checkout_completions SET operator_payment=COALESCE(operator_payment,'[]'::jsonb)||jsonb_build_array($3::jsonb),payment_state=CASE WHEN trusted_payment IS NULL AND $4 AND draft_id=$5 AND attempt=$6::jsonb THEN 'PAID' ELSE payment_state END,trusted_payment=CASE WHEN trusted_payment IS NULL AND $4 AND draft_id=$5 AND attempt=$6::jsonb THEN $7::jsonb ELSE trusted_payment END,completion_state='OPERATOR',last_error='Payment association requires operator review',lease_token=NULL,lease_until=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND lease_token=$2 AND lease_until>CURRENT_TIMESTAMP")
        .bind(id).bind(token).bind(encode(payment)?).bind(paid).bind(payment.draft_id.as_ref()).bind(encode(&payment.attempt)?).bind(encode(&payment.state)?).execute(pool).await?.rows_affected();
    owned(count)
}
#[cfg(test)]
pub(crate) async fn aliases_for_checkout(
    pool: &PgPool,
    id: Uuid,
) -> Result<Vec<PaymentAlias>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT kind,value FROM checkout_payment_aliases WHERE checkout_id=$1 ORDER BY kind",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(kind, value)| {
            Ok(PaymentAlias {
                kind: decode_code(&kind)?,
                value,
            })
        })
        .collect()
}
pub(crate) struct AliasMapping {
    pub completion_state: CompletionState,
    pub attempt: PaymentAttempt,
    pub checkout_id: Uuid,
    pub account: String,
    pub environment: PaymentEnvironment,
    pub kind: AliasKind,
    pub value: String,
    pub order_id: Option<ShopifyId>,
    pub snapshot: Snapshot,
}
#[derive(FromRow)]
struct StoredAliasMapping {
    completion_state: String,
    attempt: String,
    checkout_id: Uuid,
    account: String,
    environment: String,
    kind: String,
    value: String,
    order_id: Option<String>,
    snapshot: String,
}
pub(crate) async fn lookup_aliases(
    pool: &PgPool,
    provider: Provider,
    references: &[String],
) -> Result<Vec<AliasMapping>, AppError> {
    let rows:Vec<StoredAliasMapping>=sqlx::query_as("SELECT a.checkout_id,a.account,a.environment,a.kind,a.value,c.order_id,c.snapshot::text,c.attempt::text,c.completion_state FROM checkout_payment_aliases a JOIN checkout_completions c ON c.id=a.checkout_id WHERE a.provider=$1 AND a.value=ANY($2)")
        .bind(code(provider)?).bind(references).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            Ok(AliasMapping {
                completion_state: decode_code(&row.completion_state)?,
                attempt: decode(&row.attempt)?,
                checkout_id: row.checkout_id,
                account: row.account,
                environment: decode_code(&row.environment)?,
                kind: decode_code(&row.kind)?,
                value: row.value,
                order_id: row
                    .order_id
                    .map(ShopifyId::new)
                    .transpose()
                    .map_err(|_| saved_invalid())?,
                snapshot: decode(&row.snapshot)?,
            })
        })
        .collect()
}

#[derive(FromRow)]
struct StoredRow {
    id: Uuid,
    snapshot: String,
    attempt: String,
    guest_token_hash: Vec<u8>,
    guest_token_expires_at: DateTime<Utc>,
    guest_token_revoked_at: Option<DateTime<Utc>>,
    next_verify_at: Option<DateTime<Utc>>,
    payment_state: String,
    completion_state: String,
    trusted_payment: Option<String>,
    order_id: Option<String>,
    completion_intent: bool,
    lease_token: Option<Uuid>,
    lease_until: Option<DateTime<Utc>>,
}
impl StoredRow {
    fn decode(self) -> Result<CompletionRow, AppError> {
        Ok(CompletionRow {
            id: self.id,
            snapshot: decode(&self.snapshot)?,
            attempt: decode(&self.attempt)?,
            guest_token_hash: self.guest_token_hash,
            guest_token_expires_at: self.guest_token_expires_at,
            guest_token_revoked_at: self.guest_token_revoked_at,
            next_verify_at: self.next_verify_at,
            payment_state: decode_code(&self.payment_state)?,
            completion_state: decode_code(&self.completion_state)?,
            trusted_payment: self.trusted_payment.as_deref().map(decode).transpose()?,
            order_id: self
                .order_id
                .map(ShopifyId::new)
                .transpose()
                .map_err(|_| saved_invalid())?,
            completion_intent: self.completion_intent,
            lease_token: self.lease_token,
            lease_until: self.lease_until,
        })
    }
}
fn owned(count: u64) -> Result<(), AppError> {
    if count == 1 {
        Ok(())
    } else {
        Err(conflict("Completion ownership expired"))
    }
}
fn encode(value: &impl Serialize) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| saved_invalid())
}
fn code(value: impl Serialize) -> Result<String, AppError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(saved_invalid)
}
fn decode<T: DeserializeOwned>(value: &str) -> Result<T, AppError> {
    serde_json::from_str(value).map_err(|_| saved_invalid())
}
fn decode_code<T: DeserializeOwned>(value: &str) -> Result<T, AppError> {
    serde_json::from_value(serde_json::Value::String(value.into())).map_err(|_| saved_invalid())
}
fn saved_invalid() -> AppError {
    AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "Saved checkout metadata is invalid",
    )
}
#[cfg(test)]
#[path = "../tests/repositories/checkout_completions.rs"]
mod tests;
