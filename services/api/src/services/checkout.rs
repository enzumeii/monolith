mod provider;
mod purchase;

use crate::{
    client::{paypal::PaypalClient, shopify::ShopifyClient, stripe::StripeClient},
    config::CheckoutSettings,
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories::checkout as repo,
};
use axum::http::StatusCode;
use chrono::Utc;
use domain::dto::checkout::{
    CheckoutNextAction, CheckoutOrder, CheckoutProvider, CheckoutResponse, CheckoutStatus,
    CreateCheckoutRequest,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) struct CheckoutClients<'a> {
    pub shopify: &'a ShopifyClient,
    pub stripe: Option<&'a StripeClient>,
    pub paypal: Option<&'a PaypalClient>,
}

pub(crate) async fn create(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    request: &CreateCheckoutRequest,
    guest_hash: &[u8],
    source_hash: &[u8],
) -> Result<CheckoutResponse, AppError> {
    let id = Uuid::parse_str(&request.operation_id)
        .map_err(|_| conflict("Invalid operation identity"))?;
    let fingerprint = Sha256::digest(
        serde_json::to_vec(request).map_err(|e| AppError::from(anyhow::Error::new(e)))?,
    )
    .to_vec();
    if let Some(existing) = repo::find(pool, id).await? {
        authorize(&existing, guest_hash)?;
        if existing.request_fingerprint != fingerprint {
            return Err(conflict(
                "Checkout operation was used with different purchase details",
            ));
        }
        return advance(pool, settings, clients, id, false).await;
    }
    let mut transaction = pool.begin().await?;
    let allowed = repo::rate_limit(
        &mut transaction,
        source_hash,
        i64::try_from(settings.rate_limit_window_secs)
            .map_err(|_| conflict("Invalid rate limit duration"))?,
        i64::from(settings.rate_limit_attempts),
    )
    .await?;
    transaction.commit().await?;
    if !allowed {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many checkout attempts",
        ));
    }
    let checkout =
        purchase::prepare(pool, settings, clients, request, guest_hash, fingerprint).await?;
    let mut transaction = pool.begin().await?;
    repo::insert(&mut transaction, &checkout).await?;
    transaction.commit().await?;
    let stored = repo::find(pool, id)
        .await?
        .ok_or_else(|| conflict("Checkout could not be recovered"))?;
    authorize(&stored, guest_hash)?;
    if stored.request_fingerprint != checkout.request_fingerprint {
        return Err(conflict(
            "Checkout operation was used with different purchase details",
        ));
    }
    advance(pool, settings, clients, id, false).await
}

pub(crate) async fn verify(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    id: Uuid,
    guest_hash: &[u8],
) -> Result<CheckoutResponse, AppError> {
    let checkout = repo::find(pool, id).await?.ok_or_else(unauthorized)?;
    authorize(&checkout, guest_hash)?;
    advance(pool, settings, clients, id, false).await
}

pub(crate) async fn webhook(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    provider: CheckoutProvider,
    event_id: &str,
    resource_id: Option<&str>,
    correlation_id: Option<Uuid>,
) -> Result<(), AppError> {
    let name = provider_name(provider);
    let associated = if let Some(resource) = resource_id {
        repo::by_resource(pool, name, resource).await?
    } else {
        None
    };
    let mut checkout = match associated {
        Some(row) => row,
        None => match correlation_id {
            Some(id) => repo::find(pool, id).await?,
            None => None,
        }
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout association is not available yet",
            )
        })?,
    };
    if checkout.provider != name || correlation_id.is_some_and(|id| id != checkout.id) {
        return Err(conflict("Webhook association conflicts"));
    }
    if checkout.provider_resource_id.is_none() {
        let resource = resource_id.ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Payment association is not available yet",
            )
        })?;
        let token = Uuid::new_v4();
        let Some(mut claimed) = repo::claim(pool, checkout.id, token).await? else {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout is being advanced",
            ));
        };
        let result =
            provider::bind_authenticated_resource(pool, clients, &mut claimed, token, resource)
                .await;
        let release = repo::release(pool, claimed.id, token).await;
        result?;
        release?;
        checkout = claimed;
    }
    if let Some(resource) = resource_id {
        let stripe_intent = provider == CheckoutProvider::Stripe
            && resource.starts_with("pi_")
            && correlation_id == Some(checkout.id)
            && checkout
                .provider_resource_id
                .as_deref()
                .is_some_and(|id| id.starts_with("cs_"));
        if !stripe_intent && checkout.provider_resource_id.as_deref() != Some(resource) {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Payment association is not available yet",
            ));
        }
    }
    let mut transaction = pool.begin().await?;
    let done = repo::event(&mut transaction, &checkout, event_id).await?;
    transaction.commit().await?;
    if done {
        return Ok(());
    }
    let response = advance(pool, settings, clients, checkout.id, true).await?;
    if matches!(
        response.status,
        CheckoutStatus::Processing | CheckoutStatus::ReconciliationRequired
    ) {
        return Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout advancement requires retry",
        ));
    }
    repo::finish_event(pool, &checkout, event_id).await
}

async fn advance(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    id: Uuid,
    webhook: bool,
) -> Result<CheckoutResponse, AppError> {
    let token = Uuid::new_v4();
    let Some(mut checkout) = repo::claim(pool, id, token).await? else {
        if webhook {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout is being advanced",
            ));
        }
        let row = repo::find(pool, id).await?.ok_or_else(unauthorized)?;
        return Ok(response(&row, true));
    };
    let result = advance_claimed(pool, settings, clients, &mut checkout, token).await;
    let release = repo::release(pool, id, token).await;
    result?;
    release?;
    Ok(response(&checkout, false))
}

async fn advance_claimed(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    if checkout.completion_state == CompletionState::Completed {
        return Ok(());
    }
    if matches!(
        checkout.draft_state,
        DraftState::Creating | DraftState::Uncertain
    ) {
        checkout.draft_state = DraftState::Uncertain;
        repo::save(pool, checkout, token).await?;
        if let (Some(id), Some(requested)) = (&checkout.draft_id, checkout.reservation_expires_at) {
            let id = crate::models::shopify::ShopifyId::new(id).map_err(crate::error::upstream)?;
            if let Some(draft) = clients
                .shopify
                .checkout_draft(&id)
                .await
                .map_err(crate::error::upstream)?
                && purchase::validate_draft(checkout, &draft, requested).is_ok()
            {
                checkout.draft_state = DraftState::Reserved;
                checkout.reservation_expires_at = draft.reserve_inventory_until;
                repo::save(pool, checkout, token).await?;
            }
        }
        if checkout.draft_state != DraftState::Reserved {
            return Ok(());
        }
    }
    if checkout.draft_state == DraftState::NotStarted {
        purchase::reserve(pool, settings, clients, checkout, token).await?;
    }
    if checkout.draft_state != DraftState::Reserved {
        return Ok(());
    }
    provider::advance(pool, settings, clients, checkout, token).await
}

fn authorize(checkout: &Checkout, supplied: &[u8]) -> Result<(), AppError> {
    if supplied.len() != 32
        || checkout.guest_secret_hash.len() != 32
        || Utc::now() >= checkout.owner_expires_at
    {
        return Err(unauthorized());
    }
    let mismatch = checkout
        .guest_secret_hash
        .iter()
        .zip(supplied)
        .fold(0u8, |v, (a, b)| v | (a ^ b));
    if mismatch != 0 {
        return Err(unauthorized());
    }
    Ok(())
}
fn unauthorized() -> AppError {
    AppError::new(
        StatusCode::UNAUTHORIZED,
        "Checkout credential is invalid or expired",
    )
    .with_bearer_challenge()
}
fn conflict(message: &'static str) -> AppError {
    AppError::new(StatusCode::CONFLICT, message)
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Selected payment provider is unavailable",
    )
}
fn provider_name(provider: CheckoutProvider) -> &'static str {
    match provider {
        CheckoutProvider::Stripe => "stripe",
        CheckoutProvider::Paypal => "paypal",
    }
}
fn response(checkout: &Checkout, busy: bool) -> CheckoutResponse {
    let status = if checkout.completion_state == CompletionState::Completed {
        CheckoutStatus::Completed
    } else if checkout.draft_state == DraftState::Uncertain
        || checkout.completion_state == CompletionState::Uncertain
        || (checkout.payment_state == PaymentState::Paid
            && checkout
                .reservation_expires_at
                .is_none_or(|expiry| expiry <= Utc::now()))
    {
        CheckoutStatus::ReconciliationRequired
    } else if busy {
        CheckoutStatus::Processing
    } else {
        match checkout.payment_state {
            PaymentState::RequiresAction => CheckoutStatus::RequiresAction,
            PaymentState::Unpaid => CheckoutStatus::Unpaid,
            _ => CheckoutStatus::Processing,
        }
    };
    CheckoutResponse {
        checkout_id: checkout.id.to_string(),
        status,
        money: checkout.snapshot.money.clone(),
        reservation_expires_at: checkout.reservation_expires_at.map(|v| v.to_rfc3339()),
        next_action: if status == CheckoutStatus::RequiresAction {
            checkout
                .redirect_url
                .clone()
                .map(|url| CheckoutNextAction::Redirect { url })
        } else {
            None
        },
        order: if status == CheckoutStatus::Completed {
            checkout
                .order_id
                .clone()
                .zip(checkout.order_name.clone())
                .map(|(id, name)| CheckoutOrder { id, name })
        } else {
            None
        },
    }
}

#[cfg(test)]
#[path = "../tests/services/checkout.rs"]
mod tests;
