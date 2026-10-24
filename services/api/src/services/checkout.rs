pub(crate) mod notification;
mod provider;
mod purchase;
mod recovery;

pub(crate) use recovery::RecoveryOutcome;

use crate::{
    client::{paypal::PaypalClient, shopify::ShopifyClient, stripe::StripeClient},
    config::CheckoutSettings,
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories::checkout as repo,
};
use axum::http::StatusCode;
use chrono::{SubsecRound, Utc};
use domain::dto::checkout::{
    CheckoutMoney, CheckoutNextAction, CheckoutOrder, CheckoutProvider, CheckoutResponse,
    CheckoutStatus, CreateCheckoutRequest,
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
) -> Result<CheckoutResponse, AppError> {
    let id = Uuid::parse_str(&request.operation_id)
        .map_err(|_| conflict("Invalid operation identity"))?;
    let fingerprint = Sha256::digest(
        serde_json::to_vec(request).map_err(|error| AppError::from(anyhow::Error::new(error)))?,
    )
    .to_vec();
    let existing = repo::find(pool, id).await?;
    if let Some(row) = &existing {
        authorize(row, guest_hash)?;
        if row.request_fingerprint != fingerprint {
            return Err(conflict(
                "Checkout operation was used with different purchase details",
            ));
        }
        if row.draft_state != DraftState::NotStarted {
            return advance(pool, clients, id, false, None, settings).await;
        }
    }
    let (checkout, prepared) =
        purchase::prepare(pool, settings, clients, request, guest_hash, fingerprint).await?;
    let mut transaction = pool.begin().await?;
    repo::insert(&mut transaction, &checkout).await?;
    transaction.commit().await?;
    let stored = repo::find(pool, id)
        .await?
        .ok_or_else(|| conflict("Checkout could not be recovered"))?;
    authorize(&stored, guest_hash)?;
    if stored.request_fingerprint != checkout.request_fingerprint
        || stored.evidence_hash != checkout.evidence_hash
        || stored.required_tag_hashes != checkout.required_tag_hashes
    {
        return Err(conflict(
            "Checkout operation was used with different purchase details",
        ));
    }
    advance(pool, clients, id, false, Some(&prepared), settings).await
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
    advance(pool, clients, id, false, None, settings).await
}

pub(crate) async fn webhook(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    provider: CheckoutProvider,
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
        let result = async {
            let draft = purchase::read(clients, &claimed).await?;
            let money = purchase::validate_purchase(&claimed, &draft)?;
            provider::bind_authenticated_resource(
                pool,
                clients,
                &mut claimed,
                &money,
                token,
                resource,
            )
            .await
        }
        .await;
        let release = repo::release(pool, claimed.id, token).await;
        result.map_err(AdvanceError::into_app)?;
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
    let response = advance(pool, clients, checkout.id, true, None, settings).await?;
    if matches!(
        response.status,
        CheckoutStatus::Processing | CheckoutStatus::ReconciliationRequired
    ) {
        return Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout advancement requires retry",
        ));
    }
    Ok(())
}

pub(crate) async fn recover_claimed(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<RecoveryOutcome, AppError> {
    match advance_claimed(pool, settings, clients, checkout, token, None, false).await {
        Ok(outcome) => Ok(outcome),
        Err(AdvanceError::Business(error)) => Ok(recovery::classify(&error)),
        Err(AdvanceError::Persistence(error)) => Err(error),
    }
}

async fn advance(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    id: Uuid,
    webhook: bool,
    prepared: Option<&purchase::PreparedPurchase>,
    settings: &CheckoutSettings,
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
        if row.draft_id.is_none() {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout purchase is being prepared",
            ));
        }
        return hydrate_response(clients, &row, true).await;
    };
    let result = async {
        advance_claimed(
            pool,
            settings,
            clients,
            &mut checkout,
            token,
            prepared,
            true,
        )
        .await
        .map_err(AdvanceError::into_app)?;
        hydrate_response(clients, &checkout, false).await
    }
    .await;
    let release = repo::release(pool, id, token).await;
    let result = result?;
    release?;
    Ok(result)
}

async fn advance_claimed(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    prepared: Option<&purchase::PreparedPurchase>,
    report_diagnostics: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    if matches!(
        checkout.draft_state,
        DraftState::Creating | DraftState::Uncertain
    ) {
        purchase::recover(pool, clients, checkout, token).await?;
    }
    if checkout.draft_state == DraftState::NotStarted {
        let prepared = prepared.ok_or_else(|| {
            recovery::manual(
                conflict("Original purchase request is required before draft creation"),
                crate::models::checkout::RecoveryCode::MissingPurchase,
            )
        })?;
        purchase::reserve(pool, settings, clients, checkout, prepared, token).await?;
    }
    let draft = purchase::read(clients, checkout).await?;
    let money = purchase::validate_purchase(checkout, &draft)?;
    if checkout.completion_state == CompletionState::Completed {
        return Ok(RecoveryOutcome::Observed(
            crate::models::checkout::RecoveryCode::Observed,
        ));
    }
    let reservation_safe = purchase::validate_reservation(checkout, &draft).is_ok()
        && checkout
            .reservation_expires_at
            .is_some_and(|expiry| expiry > Utc::now());
    provider::advance(
        pool,
        clients,
        checkout,
        &money,
        token,
        reservation_safe,
        report_diagnostics,
    )
    .await
}

enum AdvanceError {
    Business(AppError),
    Persistence(AppError),
}
impl AdvanceError {
    fn into_app(self) -> AppError {
        match self {
            Self::Business(error) | Self::Persistence(error) => error,
        }
    }
}
impl From<AppError> for AdvanceError {
    fn from(error: AppError) -> Self {
        Self::Business(error)
    }
}
async fn save(pool: &PgPool, checkout: &Checkout, token: Uuid) -> Result<(), AdvanceError> {
    repo::save(pool, checkout, token)
        .await
        .map_err(AdvanceError::Persistence)
}

async fn hydrate_response(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
    busy: bool,
) -> Result<CheckoutResponse, AppError> {
    let draft = purchase::read(clients, checkout).await?;
    let money = purchase::validate_purchase(checkout, &draft)?;
    let status = response_status(checkout, busy);
    let next_action = if status == CheckoutStatus::RequiresAction {
        let resource = checkout
            .provider_resource_id
            .as_deref()
            .ok_or_else(|| conflict("Checkout payment is missing"))?;
        let url = if checkout.provider == "stripe" {
            clients
                .stripe
                .ok_or_else(unavailable)?
                .get_checkout_session(resource)
                .await
                .map_err(|error| crate::error::upstream(error))?
                .url
        } else {
            clients
                .paypal
                .ok_or_else(unavailable)?
                .get_order(resource)
                .await
                .map_err(|error| crate::error::upstream(error))?
                .links
                .into_iter()
                .find(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
                .map(|link| link.href)
        }
        .ok_or_else(|| conflict("Checkout payment action is unavailable"))?;
        Some(CheckoutNextAction::Redirect {
            url: provider::redirect(&url)?,
        })
    } else {
        None
    };
    let order = if status == CheckoutStatus::Completed {
        let id = crate::models::shopify::ShopifyId::new(
            checkout
                .order_id
                .as_deref()
                .ok_or_else(|| conflict("Completed checkout order is missing"))?,
        )
        .map_err(|error| crate::error::upstream(error))?;
        let order = clients
            .shopify
            .checkout_order(&id)
            .await
            .map_err(|error| crate::error::upstream(error))?
            .ok_or_else(|| conflict("Completed checkout order is missing"))?;
        let actual = &order.total_price_set.presentment_money;
        if !purchase::tags_match(checkout, &order.tags)
            || actual.currency_code != money.currency
            || purchase::exact(money.minor, &money.currency)?.parse_amount(&actual.amount)?
                != money.minor
        {
            return Err(conflict("Completed Shopify order does not match purchase"));
        }
        Some(CheckoutOrder {
            id: id.as_ref().to_owned(),
            name: order
                .name
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| conflict("Completed checkout order name is missing"))?,
        })
    } else {
        None
    };
    Ok(response(checkout, status, money, next_action, order))
}

fn response(
    checkout: &Checkout,
    status: CheckoutStatus,
    money: CheckoutMoney,
    next_action: Option<CheckoutNextAction>,
    order: Option<CheckoutOrder>,
) -> CheckoutResponse {
    CheckoutResponse {
        checkout_id: checkout.id.to_string(),
        status,
        money,
        reservation_expires_at: checkout
            .reservation_expires_at
            .map(|value| value.trunc_subsecs(6).to_rfc3339()),
        next_action,
        order,
    }
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
fn response_status(checkout: &Checkout, busy: bool) -> CheckoutStatus {
    if checkout.completion_state == CompletionState::Completed {
        CheckoutStatus::Completed
    } else if checkout.draft_state == DraftState::Uncertain
        || checkout.payment_state == PaymentState::Uncertain
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
    }
}

#[cfg(test)]
#[path = "../tests/services/checkout.rs"]
mod tests;
