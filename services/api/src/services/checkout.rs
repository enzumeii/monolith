pub(crate) mod correlation;
pub(crate) mod notification;
mod provider;
mod purchase;
mod recovery;
pub(crate) mod webhooks;

pub(crate) use recovery::RecoveryOutcome;

use crate::{
    client::{paypal::PaypalClient, shopify::ShopifyClient, stripe::StripeClient},
    config::CheckoutSettings,
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories, services, transformers,
};
use axum::http::StatusCode;
use chrono::Utc;
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
    let existing = repositories::checkout::find(pool, id).await?;
    if let Some(row) = &existing {
        authorize(row, guest_hash)?;
        if row.request_fingerprint != fingerprint {
            return Err(conflict(
                "Checkout operation was used with different purchase details",
            ));
        }
        if terminal_unpaid(row) || row.draft_state != DraftState::NotStarted {
            return advance(pool, clients, id, false, None, settings).await;
        }
    }
    let (checkout, prepared) = purchase::prepare(
        pool,
        settings,
        clients,
        request,
        guest_hash,
        fingerprint,
        existing.as_ref(),
    )
    .await?;
    let mut transaction = pool.begin().await?;
    repositories::checkout::insert(&mut transaction, &checkout).await?;
    transaction.commit().await?;
    let stored = repositories::checkout::find(pool, id)
        .await?
        .ok_or_else(|| conflict("Checkout could not be recovered"))?;
    authorize(&stored, guest_hash)?;
    if stored.completion_version != checkout.completion_version
        || stored.correlation_version != checkout.correlation_version
        || stored.request_fingerprint != checkout.request_fingerprint
        || stored.provider_account_id != checkout.provider_account_id
        || stored.provider_environment != checkout.provider_environment
        || stored.payment_amount_minor != checkout.payment_amount_minor
        || stored.payment_currency != checkout.payment_currency
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
    let checkout = repositories::checkout::find(pool, id)
        .await?
        .ok_or_else(unauthorized)?;
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
        repositories::checkout::by_resource(pool, name, resource).await?
    } else {
        None
    };
    let mut checkout = match associated {
        Some(row) => row,
        None => match correlation_id {
            Some(id) => repositories::checkout::find(pool, id).await?,
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
        let Some(mut claimed) = repositories::checkout::claim(pool, checkout.id, token).await?
        else {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout is being advanced",
            ));
        };
        let result = async {
            let draft = purchase::read(clients, &claimed).await?;
            let draft_money = purchase::validate_purchase(&claimed, &draft)?;
            let money = purchase::payment_money(&claimed, &draft_money)?;
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
        let release = repositories::checkout::release(pool, claimed.id, token).await;
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
    let Some(mut checkout) = repositories::checkout::claim(pool, id, token).await? else {
        if webhook {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Checkout is being advanced",
            ));
        }
        let row = repositories::checkout::find(pool, id)
            .await?
            .ok_or_else(unauthorized)?;
        if row.draft_id.is_none() && !terminal_unpaid(&row) {
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
    let release = repositories::checkout::release(pool, id, token).await;
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
    if terminal_unpaid(checkout) {
        return Ok(RecoveryOutcome::Observed(
            crate::models::checkout::RecoveryCode::Observed,
        ));
    }
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
        if terminal_unpaid(checkout) {
            return Ok(RecoveryOutcome::Observed(
                crate::models::checkout::RecoveryCode::Observed,
            ));
        }
    }
    let draft = purchase::read(clients, checkout).await?;
    let shopify_money = purchase::validate_purchase(checkout, &draft)?;
    let money = purchase::payment_money(checkout, &shopify_money)?;
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
    repositories::checkout::save(pool, checkout, token)
        .await
        .map_err(AdvanceError::Persistence)
}

async fn hydrate_response(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
    busy: bool,
) -> Result<CheckoutResponse, AppError> {
    if terminal_unpaid(checkout) && checkout.correlation_version == correlation::INDEPENDENT_VERSION
    {
        let money = correlation::payment_money(checkout)?
            .ok_or_else(|| conflict("Checkout payment snapshot is missing"))?;
        return Ok(transformers::checkout::response(
            checkout,
            CheckoutStatus::Unpaid,
            money,
            None,
            None,
        ));
    }
    let draft = purchase::read(clients, checkout).await?;
    let shopify_money = purchase::validate_purchase(checkout, &draft)?;
    let money = purchase::payment_money(checkout, &shopify_money)?;
    let status = response_status(checkout, busy);
    let next_action = next_action(clients, checkout, status).await?;
    let order = completed_order(clients, checkout, &shopify_money, status).await?;
    Ok(transformers::checkout::response(
        checkout,
        status,
        money,
        next_action,
        order,
    ))
}

async fn next_action(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
    status: CheckoutStatus,
) -> Result<Option<CheckoutNextAction>, AppError> {
    if status != CheckoutStatus::RequiresAction {
        return Ok(None);
    }
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
    Ok(Some(CheckoutNextAction::Redirect {
        url: provider::redirect(&url)?,
    }))
}

async fn completed_order(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
    money: &CheckoutMoney,
    status: CheckoutStatus,
) -> Result<Option<CheckoutOrder>, AppError> {
    if status != CheckoutStatus::Completed {
        return Ok(None);
    }
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
    if checkout.completion_version == 2 {
        services::checkout_completion::validate_external_paid_order(
            &order,
            &purchase::exact(money.minor, &money.currency)?,
            &services::checkout_completion::ExternalPaymentRecording::from_checkout(checkout)?,
            if checkout.provider == "stripe" {
                "Stripe Card Payments"
            } else {
                "paypal"
            },
        )?;
    }
    let actual = &order.total_price_set.presentment_money;
    if !purchase::tags_match(checkout, &order.tags)
        || actual.currency_code != money.currency
        || purchase::exact(money.minor, &money.currency)?.parse_amount(&actual.amount)?
            != money.minor
    {
        return Err(conflict("Completed Shopify order does not match purchase"));
    }
    Ok(Some(CheckoutOrder {
        id: id.as_ref().to_owned(),
        name: order
            .name
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| conflict("Completed checkout order name is missing"))?,
    }))
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
fn terminal_unpaid(checkout: &Checkout) -> bool {
    checkout.payment_state == PaymentState::Unpaid
        && checkout.provider_resource_id.is_none()
        && checkout.completion_state == CompletionState::NotStarted
        && checkout.payment_recording_state
            == crate::models::checkout::PaymentRecordingState::NotStarted
}
fn response_status(checkout: &Checkout, busy: bool) -> CheckoutStatus {
    if terminal_unpaid(checkout) {
        CheckoutStatus::Unpaid
    } else if checkout.completion_state == CompletionState::Completed {
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
