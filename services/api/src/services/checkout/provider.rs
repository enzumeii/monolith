use super::{
    AdvanceError, CheckoutClients, RecoveryOutcome, conflict, purchase, recovery, unavailable,
};
use crate::{
    error::AppError,
    models::checkout::{
        Checkout, CompletionState, DraftState, PaymentRecordingState, PaymentState, RecoveryCode,
    },
};
use crate::{
    error::upstream,
    models::{
        paypal::{
            self, CreateOrderRequest, ExperienceContext, OrderIntent, OrderStatus, Payee,
            PaymentMoney, PaymentSource, PaypalPaymentSource, PurchaseUnitRequest,
        },
        shopify::ShopifyId,
        stripe::{self, CheckoutMode, CreateCheckoutSession},
    },
    services::{
        self,
        checkout_completion::{CompletionMode, CompletionOutcome, PaymentContext},
    },
};
use chrono::{Duration, Utc};
use domain::dto::checkout::CheckoutMoney;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) async fn advance(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    reservation_safe: bool,
    report_diagnostics: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    validate_version_combination(checkout)?;
    if checkout.provider_resource_id.is_none() {
        if !reservation_safe {
            checkout.payment_state = if checkout.payment_state == PaymentState::NotStarted {
                PaymentState::Unpaid
            } else {
                PaymentState::Uncertain
            };
            super::save(pool, checkout, token).await?;
            return Ok(RecoveryOutcome::Manual(RecoveryCode::ReservationUnsafe));
        }
        let retention = if checkout.provider == "stripe" {
            Duration::hours(23)
        } else {
            Duration::hours(5)
        };
        if checkout.payment_state != PaymentState::NotStarted
            && Utc::now() >= checkout.created_at + retention
        {
            checkout.payment_state = PaymentState::Uncertain;
            super::save(pool, checkout, token).await?;
            return Ok(RecoveryOutcome::Manual(RecoveryCode::MutationWindowExpired));
        }
        let initiation = if checkout.payment_state == PaymentState::NotStarted {
            InitiationMode::Fresh
        } else {
            InitiationMode::Recover
        };
        if initiation == InitiationMode::Recover
            && (checkout.gateway_request_fingerprint.is_none()
                || checkout.success_url.is_none()
                || checkout.cancel_url.is_none())
        {
            return Ok(RecoveryOutcome::Manual(RecoveryCode::MissingPurchase));
        }
        let fingerprint = initiation_fingerprint(checkout, money)?;
        if checkout
            .gateway_request_fingerprint
            .as_ref()
            .is_some_and(|stored| stored != &fingerprint)
        {
            return Err(crate::models::checkout_completion::evidence_conflict(
                "Checkout gateway parameters changed; reconciliation is required",
            )
            .into());
        }
        checkout.gateway_request_fingerprint = Some(fingerprint);
        checkout.payment_state = PaymentState::Creating;
        super::save(pool, checkout, token).await?;
        let outcome = start_or_recover_payment(clients, checkout, money, initiation).await?;
        checkout.payment_state = match &outcome {
            InitiationOutcome::RequiresAction => PaymentState::RequiresAction,
            InitiationOutcome::Reconcile => PaymentState::Processing,
            InitiationOutcome::Rejected(error) => {
                checkout.payment_state = PaymentState::Unpaid;
                super::save(pool, checkout, token).await?;
                crate::error::log_background_error("Checkout payment rejected", error);
                return Ok(RecoveryOutcome::Observed(RecoveryCode::Observed));
            }
        };
        checkout.success_url = None;
        checkout.cancel_url = None;
        super::save(pool, checkout, token).await?;
        if matches!(outcome, InitiationOutcome::RequiresAction) {
            return Ok(RecoveryOutcome::Observed(RecoveryCode::WaitingForPayment));
        }
    }
    if checkout.provider == "stripe" {
        reconcile_stripe_payment(
            pool,
            clients,
            checkout,
            money,
            token,
            reservation_safe,
            report_diagnostics,
        )
        .await
    } else {
        reconcile_paypal_payment(
            pool,
            clients,
            checkout,
            money,
            token,
            reservation_safe,
            report_diagnostics,
        )
        .await
    }
}

pub(super) async fn bind_authenticated_resource(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    resource: &str,
) -> Result<(), AdvanceError> {
    validate_version_combination(checkout)?;
    if !matches!(
        checkout.payment_state,
        PaymentState::Creating | PaymentState::Uncertain
    ) || checkout.draft_state != DraftState::Reserved
        || checkout.provider_resource_id.is_some()
    {
        return Err(conflict("Payment initiation is not awaiting association").into());
    }
    checkout.provider_resource_id = Some(resource.to_owned());
    if checkout.provider == "stripe" {
        if !resource.starts_with("cs_") {
            return Err(conflict("Stripe checkout resource is invalid").into());
        }
        let client = clients.stripe.ok_or_else(unavailable)?;
        services::payments::stripe::verify_payment(client, &stripe_expected(checkout, money)?)
            .await
            .map_err(|error| upstream(error))?;
    } else {
        let client = clients.paypal.ok_or_else(unavailable)?;
        services::payments::paypal::verify_payment(client, &paypal_expected(checkout, money)?)
            .await
            .map_err(|error| upstream(error))?;
        let order = client
            .get_order(resource)
            .await
            .map_err(|error| upstream(error))?;
        order
            .links
            .iter()
            .find(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
            .map(|link| redirect(&link.href))
            .transpose()?;
    }
    checkout.payment_state = PaymentState::RequiresAction;
    checkout.success_url = None;
    checkout.cancel_url = None;
    super::save(pool, checkout, token).await
}

pub(super) fn paypal_amount(money: &CheckoutMoney) -> Result<String, AppError> {
    let minor = paypal_minor(money)?;
    let precision = paypal::payment::currency_scale(&money.currency)
        .ok_or_else(|| conflict("Unsupported PayPal currency"))?;
    Ok(purchase::format_minor(
        minor,
        u8::try_from(precision).map_err(|_| conflict("Invalid PayPal precision"))?,
    ))
}

pub(super) fn redirect(value: &str) -> Result<String, AppError> {
    let url =
        reqwest::Url::parse(value).map_err(|_| conflict("Payment redirect URL is invalid"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(conflict("Payment redirect URL is invalid"));
    }
    Ok(url.into())
}

pub(super) fn initiation_fingerprint(
    checkout: &Checkout,
    money: &CheckoutMoney,
) -> Result<Vec<u8>, AppError> {
    let value = (
        &checkout.provider,
        &checkout.provider_account_id,
        &checkout.provider_environment,
        checkout.id,
        money,
        checkout
            .reservation_expires_at
            .map(|value| value.timestamp()),
        &checkout.success_url,
        &checkout.cancel_url,
        "Convention preorder",
    );
    Ok(Sha256::digest(
        serde_json::to_vec(&value).map_err(|error| AppError::from(anyhow::Error::new(error)))?,
    )
    .to_vec())
}

fn validate_version_combination(checkout: &Checkout) -> Result<(), AppError> {
    match (checkout.completion_version, checkout.correlation_version) {
        (1 | 2, super::correlation::LEGACY_VERSION)
        | (2, super::correlation::METADATA_VERSION | super::correlation::INDEPENDENT_VERSION) => {
            Ok(())
        }
        _ => Err(crate::models::checkout_completion::evidence_conflict(
            "Checkout completion and correlation versions are unsupported",
        )),
    }
}

async fn reconcile_stripe_payment(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    reservation_safe: bool,
    report_diagnostics: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    let client = clients.stripe.ok_or_else(unavailable)?;
    let expected = stripe_expected(checkout, money)?;
    let outcome = services::payments::stripe::verify_payment(client, &expected)
        .await
        .map_err(|error| upstream(error))?;
    if checkout.payment_state == PaymentState::Paid
        && !matches!(&outcome, stripe::payment::PaymentOutcome::VerifiedPaid(_))
    {
        checkout.completion_state = CompletionState::Uncertain;
        super::save(pool, checkout, token).await?;
        return Ok(RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict));
    }
    match outcome {
        stripe::payment::PaymentOutcome::RequiresAction => {
            return pending(pool, checkout, token, reservation_safe).await;
        }
        stripe::payment::PaymentOutcome::Pending => {
            return pending(pool, checkout, token, false).await;
        }
        stripe::payment::PaymentOutcome::ExpiredUnpaid => {
            checkout.payment_state = PaymentState::Unpaid;
            super::save(pool, checkout, token).await?;
            return Ok(RecoveryOutcome::Observed(RecoveryCode::Observed));
        }
        stripe::payment::PaymentOutcome::VerifiedPaid(paid) => {
            checkout.payment_state = PaymentState::Paid;
            super::save(pool, checkout, token).await?;
            if paid.refunded || paid.amount_refunded != 0 || paid.disputed {
                checkout.completion_state = CompletionState::Uncertain;
                super::save(pool, checkout, token).await?;
                return Ok(RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict));
            }
        }
    }
    complete(
        pool,
        clients,
        checkout,
        token,
        reservation_safe,
        report_diagnostics,
        PaymentContext::Stripe {
            client,
            expected: &expected,
        },
    )
    .await
}
async fn reconcile_paypal_payment(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    reservation_safe: bool,
    report_diagnostics: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    let client = clients.paypal.ok_or_else(unavailable)?;
    let expected = paypal_expected(checkout, money)?;
    let mut outcome = services::payments::paypal::verify_payment(client, &expected)
        .await
        .map_err(|error| upstream(error))?;
    if checkout.payment_state == PaymentState::Paid
        && !matches!(&outcome, paypal::payment::PaymentOutcome::VerifiedPaid(_))
    {
        checkout.completion_state = CompletionState::Uncertain;
        super::save(pool, checkout, token).await?;
        return Ok(RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict));
    }
    if matches!(outcome, paypal::payment::PaymentOutcome::Pending) {
        let order = client
            .get_order(&expected.order_id)
            .await
            .map_err(|error| upstream(error))?;
        if order.status == OrderStatus::Approved {
            if let Some(outcome) =
                checkpoint_paypal_capture(pool, clients, checkout, token, reservation_safe).await?
            {
                return Ok(outcome);
            }
            client
                .capture_order(&expected.order_id, &format!("capture-{}", checkout.id))
                .await
                .map_err(|error| upstream(error))?;
            outcome = services::payments::paypal::verify_payment(client, &expected)
                .await
                .map_err(|error| upstream(error))?;
        }
    }
    match outcome {
        paypal::payment::PaymentOutcome::Pending => {
            let actionable =
                reservation_safe && checkout.payment_state == PaymentState::RequiresAction;
            return pending(pool, checkout, token, actionable).await;
        }
        paypal::payment::PaymentOutcome::TerminalUnpaid => {
            checkout.payment_state = PaymentState::Unpaid;
            super::save(pool, checkout, token).await?;
            return Ok(RecoveryOutcome::Observed(RecoveryCode::Observed));
        }
        paypal::payment::PaymentOutcome::VerifiedPaid(_) => {
            checkout.payment_state = PaymentState::Paid;
            super::save(pool, checkout, token).await?;
        }
    }
    complete(
        pool,
        clients,
        checkout,
        token,
        reservation_safe,
        report_diagnostics,
        PaymentContext::Paypal {
            client,
            expected: &expected,
        },
    )
    .await
}

async fn checkpoint_paypal_capture(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    reservation_safe: bool,
) -> Result<Option<RecoveryOutcome>, AdvanceError> {
    if !reservation_safe {
        return pending(pool, checkout, token, false).await.map(Some);
    }
    let current = purchase::read(clients, checkout).await?;
    if purchase::validate_reservation(checkout, &current).is_err()
        || checkout
            .reservation_expires_at
            .is_none_or(|expiry| expiry <= Utc::now())
    {
        return pending(pool, checkout, token, false).await.map(Some);
    }
    if Utc::now() >= checkout.created_at + Duration::hours(5) {
        checkout.payment_state = PaymentState::Uncertain;
        super::save(pool, checkout, token).await?;
        return Ok(Some(RecoveryOutcome::Manual(
            RecoveryCode::MutationWindowExpired,
        )));
    }
    checkout.payment_state = PaymentState::Processing;
    super::save(pool, checkout, token).await?;
    if checkout
        .reservation_expires_at
        .is_none_or(|expiry| expiry <= Utc::now())
    {
        return Ok(Some(RecoveryOutcome::Observed(
            RecoveryCode::WaitingForPayment,
        )));
    }
    Ok(None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InitiationMode {
    Fresh,
    Recover,
}

enum InitiationOutcome {
    RequiresAction,
    Reconcile,
    Rejected(AppError),
}

async fn pending(
    pool: &PgPool,
    checkout: &mut Checkout,
    token: Uuid,
    actionable: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    checkout.payment_state = if actionable {
        PaymentState::RequiresAction
    } else {
        PaymentState::Processing
    };
    super::save(pool, checkout, token).await?;
    Ok(RecoveryOutcome::Observed(RecoveryCode::WaitingForPayment))
}

async fn start_or_recover_payment(
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    mode: InitiationMode,
) -> Result<InitiationOutcome, AppError> {
    let success = checkout
        .success_url
        .clone()
        .ok_or_else(|| conflict("Checkout return URL is missing"))?;
    let cancel = checkout
        .cancel_url
        .clone()
        .ok_or_else(|| conflict("Checkout cancel URL is missing"))?;
    if checkout.provider == "stripe" {
        initiate_stripe(clients, checkout, money, mode, success, cancel).await
    } else {
        initiate_paypal(clients, checkout, money, mode, success, cancel).await
    }
}

fn initiation_rejection(
    error: crate::client::ClientError,
    mode: InitiationMode,
) -> Result<InitiationOutcome, AppError> {
    let definitive = mode == InitiationMode::Fresh
        && matches!(
            error.kind(),
            crate::client::ClientErrorKind::Http {
                status: 400 | 401 | 403 | 422,
                ..
            }
        );
    let error = upstream(error);
    if definitive {
        Ok(InitiationOutcome::Rejected(error))
    } else {
        Err(error)
    }
}

async fn initiate_stripe(
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    mode: InitiationMode,
    success: String,
    cancel: String,
) -> Result<InitiationOutcome, AppError> {
    let client = clients.stripe.ok_or_else(unavailable)?;
    let expiry = checkout
        .reservation_expires_at
        .ok_or_else(|| conflict("Reservation is missing"))?;
    if mode == InitiationMode::Fresh && expiry < Utc::now() + Duration::minutes(30) {
        return Err(conflict("Reservation is too short for Stripe checkout"));
    }
    let account = client
        .get_account()
        .await
        .map_err(|error| upstream(error))?;
    if account.id != checkout.provider_account_id || account.object != "account" {
        return Err(crate::models::checkout_completion::evidence_conflict(
            "Stripe account does not match checkout",
        ));
    }
    purchase::validate_stripe_minimum(account.default_currency.as_deref(), money)?;
    let request = CreateCheckoutSession {
        amount: money.minor,
        currency: money.currency.clone(),
        product_name: "Convention preorder".into(),
        success_url: success,
        cancel_url: cancel,
        idempotency_key: format!("checkout-{}", checkout.id),
        expires_at: Some(expiry.timestamp()),
        metadata: metadata(checkout),
    };
    let session = match match mode {
        InitiationMode::Fresh => client.create_checkout_session(&request).await,
        InitiationMode::Recover => client.recover_checkout_session(&request).await,
    } {
        Ok(session) => session,
        Err(error) => return initiation_rejection(error, mode),
    };
    if session.object != "checkout.session"
        || session.mode != CheckoutMode::Payment
        || session.livemode.to_string() != checkout.provider_environment
        || session.amount_total != Some(money.minor)
        || session.currency.as_deref() != Some(money.currency.to_ascii_lowercase().as_str())
        || session.metadata != metadata(checkout)
        || session
            .expires_at
            .is_none_or(|actual| actual > expiry.timestamp())
    {
        return Err(crate::models::checkout_completion::evidence_conflict(
            "Stripe checkout does not match purchase",
        ));
    }
    if let Some(url) = session.url.as_deref() {
        redirect(url)?;
    }
    let outcome = if session.status == stripe::CheckoutStatus::Open
        && session.payment_status == stripe::CheckoutPaymentStatus::Unpaid
    {
        InitiationOutcome::RequiresAction
    } else {
        InitiationOutcome::Reconcile
    };
    checkout.provider_resource_id = Some(session.id);
    Ok(outcome)
}

async fn initiate_paypal(
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    mode: InitiationMode,
    success: String,
    cancel: String,
) -> Result<InitiationOutcome, AppError> {
    let client = clients.paypal.ok_or_else(unavailable)?;
    if client.api_origin() != checkout.provider_environment {
        return Err(crate::models::checkout_completion::evidence_conflict(
            "PayPal environment does not match checkout",
        ));
    }
    let order = match client
        .create_order(
            &format!("checkout-{}", checkout.id),
            &CreateOrderRequest {
                intent: OrderIntent::Capture,
                purchase_units: vec![PurchaseUnitRequest {
                    reference_id: checkout.id.to_string(),
                    custom_id: checkout.id.to_string(),
                    amount: PaymentMoney {
                        currency_code: money.currency.clone(),
                        value: paypal_amount(money)?,
                    },
                    payee: Payee {
                        merchant_id: checkout.provider_account_id.clone(),
                    },
                }],
                payment_source: Some(PaymentSource {
                    paypal: PaypalPaymentSource {
                        experience_context: ExperienceContext {
                            return_url: success,
                            cancel_url: cancel,
                            shipping_preference: "NO_SHIPPING".into(),
                        },
                    },
                }),
                application_context: None,
            },
        )
        .await
    {
        Ok(order) => order,
        Err(error) => return initiation_rejection(error, mode),
    };
    let expected = paypal::payment::ExpectedPayment {
        order_id: order.id.clone(),
        merchant_id: checkout.provider_account_id.clone(),
        api_origin: checkout.provider_environment.clone(),
        reference_id: checkout.id.to_string(),
        custom_id: checkout.id.to_string(),
        amount: paypal_minor(money)?,
        currency: money.currency.clone(),
    };
    services::payments::paypal::verify_payment(client, &expected)
        .await
        .map_err(|error| upstream(error))?;
    let outcome = if matches!(
        order.status,
        OrderStatus::Created | OrderStatus::Saved | OrderStatus::PayerActionRequired
    ) {
        let link = order
            .links
            .iter()
            .find(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
            .ok_or_else(|| conflict("PayPal approval URL is missing"))?;
        redirect(&link.href)?;
        InitiationOutcome::RequiresAction
    } else {
        InitiationOutcome::Reconcile
    };
    checkout.provider_resource_id = Some(order.id);
    Ok(outcome)
}

async fn complete(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    reservation_safe: bool,
    report_diagnostics: bool,
    payment: PaymentContext<'_>,
) -> Result<RecoveryOutcome, AdvanceError> {
    let draft = ShopifyId::new(
        checkout
            .draft_id
            .as_ref()
            .ok_or_else(|| conflict("Checkout draft is missing"))?,
    )
    .map_err(|error| upstream(error))?;
    let mode = completion_mode(checkout, reservation_safe);
    let current = match clients.shopify.checkout_draft(&draft).await {
        Ok(Some(current)) => current,
        Ok(None) => return Ok(RecoveryOutcome::Retry(RecoveryCode::CompletionUncertain)),
        Err(error) => {
            return Ok(completion_failure(
                "Checkout draft readback",
                &upstream(error),
                report_diagnostics,
            ));
        }
    };
    if let Err(error) = purchase::validate_purchase(checkout, &current) {
        checkout.completion_state = CompletionState::Uncertain;
        super::save(pool, checkout, token).await?;
        return Ok(completion_failure(
            "Checkout purchase readback",
            &error,
            report_diagnostics,
        ));
    }
    if !reservation_safe && current.order.is_none() {
        checkout.completion_state = CompletionState::Uncertain;
        super::save(pool, checkout, token).await?;
        return Ok(RecoveryOutcome::Manual(RecoveryCode::ReservationUnsafe));
    }
    let CompletionAttempt {
        result,
        mutation_started,
    } = complete_with_checkpoint(pool, clients, checkout, token, payment, mode).await?;
    match result {
        Ok(CompletionOutcome::Completed { order_id }) => {
            save_completed_order(
                pool,
                clients,
                checkout,
                token,
                &order_id,
                report_diagnostics,
            )
            .await
        }
        result => {
            if checkout.payment_recording_state == PaymentRecordingState::Recording {
                checkout.payment_recording_state = PaymentRecordingState::Uncertain;
            }
            checkout.completion_state = if mutation_started || mode == CompletionMode::ReconcileOnly
            {
                CompletionState::Uncertain
            } else {
                CompletionState::NotStarted
            };
            super::save(pool, checkout, token).await?;
            match result {
                Err(error) => Ok(completion_failure(
                    "Checkout Shopify completion",
                    &error,
                    report_diagnostics,
                )),
                _ => Ok(RecoveryOutcome::Retry(RecoveryCode::CompletionUncertain)),
            }
        }
    }
}

struct CompletionAttempt {
    result: Result<CompletionOutcome, AppError>,
    mutation_started: bool,
}

async fn complete_with_checkpoint(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    payment: PaymentContext<'_>,
    mode: CompletionMode,
) -> Result<CompletionAttempt, AdvanceError> {
    validate_version_combination(checkout)?;
    let draft = ShopifyId::new(
        checkout
            .draft_id
            .as_ref()
            .ok_or_else(|| conflict("Checkout draft is missing"))?,
    )
    .map_err(|error| upstream(error))?;
    if checkout.completion_version == 2 {
        return complete_external_with_checkpoint(
            pool, clients, checkout, token, payment, mode, &draft,
        )
        .await;
    }
    let mut attempted = false;
    let mut checkpoint_error = None;
    let result = {
        let checkout = &mut *checkout;
        let attempted = &mut attempted;
        let checkpoint_error = &mut checkpoint_error;
        services::checkout_completion::complete_paid_draft_with_before_completion(
            clients.shopify,
            &draft,
            payment,
            mode,
            move || async move {
                let current = purchase::read(clients, checkout).await?;
                purchase::validate_reservation(checkout, &current)
                    .map_err(|error| recovery::manual(error, RecoveryCode::ReservationUnsafe))?;
                if checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(recovery::manual(
                        conflict("Checkout reservation expired before completion"),
                        RecoveryCode::ReservationUnsafe,
                    ));
                }
                checkout.completion_state = CompletionState::Completing;
                if let Err(error) = crate::repositories::checkout::save(pool, checkout, token).await
                {
                    *checkpoint_error = Some(error);
                    return Err(conflict(
                        "Checkout completion checkpoint could not be saved",
                    ));
                }
                if checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(recovery::manual(
                        conflict("Checkout reservation expired before completion"),
                        RecoveryCode::ReservationUnsafe,
                    ));
                }
                *attempted = true;
                Ok(())
            },
        )
        .await
    };
    if let Some(error) = checkpoint_error {
        return Err(AdvanceError::Persistence(error));
    }
    Ok(CompletionAttempt {
        result,
        mutation_started: attempted,
    })
}

async fn complete_external_with_checkpoint(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    payment: PaymentContext<'_>,
    mode: CompletionMode,
    draft: &ShopifyId,
) -> Result<CompletionAttempt, AdvanceError> {
    let required_tag_hashes = checkout.required_tag_hashes.clone();
    let slug = checkout
        .convention_slug
        .clone()
        .ok_or_else(|| conflict("Checkout convention binding is missing"))?;
    let resource = checkout.provider_resource_id.clone();
    let identity = match checkout.correlation_version {
        super::correlation::LEGACY_VERSION => None,
        super::correlation::METADATA_VERSION | super::correlation::INDEPENDENT_VERSION => {
            Some(super::correlation::Identity::from_checkout(checkout)?)
        }
        _ => {
            return Err(crate::models::checkout_completion::evidence_conflict(
                "Unsupported checkout correlation version",
            )
            .into());
        }
    };
    let money_snapshots =
        services::checkout_completion::ExternalPaymentRecording::from_checkout(checkout)?
            .money_snapshots;
    let recording = services::checkout_completion::ExternalPaymentRecording {
        state: checkout.payment_recording_state,
        required_tag_hashes: &required_tag_hashes,
        convention_slug: &slug,
        identity,
        provider_resource_id: resource.as_deref(),
        money_snapshots,
    };
    let mut progress = CheckoutCompletionProgress {
        pool,
        clients,
        checkout,
        token,
        attempted: false,
        checkpoint_error: None,
    };
    let result = services::checkout_completion::complete_external_paid_draft(
        clients.shopify,
        draft,
        payment,
        mode,
        recording,
        &mut progress,
    )
    .await;
    if let Some(error) = progress.checkpoint_error {
        return Err(AdvanceError::Persistence(error));
    }
    Ok(CompletionAttempt {
        result,
        mutation_started: progress.attempted,
    })
}

struct CheckoutCompletionProgress<'a, 'clients> {
    pool: &'a PgPool,
    clients: &'a CheckoutClients<'clients>,
    checkout: &'a mut Checkout,
    token: Uuid,
    attempted: bool,
    checkpoint_error: Option<AppError>,
}
impl services::checkout_completion::CompletionProgress for CheckoutCompletionProgress<'_, '_> {
    async fn before_mutation(
        &mut self,
        stage: services::checkout_completion::CompletionStage,
    ) -> Result<(), AppError> {
        let current = purchase::read(self.clients, self.checkout).await?;
        match stage {
            services::checkout_completion::CompletionStage::ConvertDraft => {
                purchase::validate_payable(&current)?;
                purchase::validate_reservation(self.checkout, &current)
                    .map_err(|error| recovery::manual(error, RecoveryCode::ReservationUnsafe))?;
                if self
                    .checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(recovery::manual(
                        conflict("Checkout reservation expired before completion"),
                        RecoveryCode::ReservationUnsafe,
                    ));
                }
                self.checkout.completion_state = CompletionState::Completing;
            }
            services::checkout_completion::CompletionStage::RecordExternalPayment => {
                if !matches!(
                    current.status,
                    crate::models::shopify::CompletionDraftStatus::Completed
                ) {
                    return Err(crate::models::checkout_completion::evidence_conflict(
                        "Checkout draft is not converted before payment recording",
                    ));
                }
                let order = current
                    .order
                    .ok_or_else(|| conflict("Converted checkout order is unavailable"))?;
                order
                    .id
                    .require_kind("Order")
                    .map_err(|error| upstream(error))?;
                if self
                    .checkout
                    .order_id
                    .as_deref()
                    .is_some_and(|id| id != order.id.as_ref())
                {
                    return Err(crate::models::checkout_completion::evidence_conflict(
                        "Checkout order identity changed before payment recording",
                    ));
                }
                if self.checkout.payment_recording_state != PaymentRecordingState::NotStarted {
                    return Err(conflict(
                        "Checkout payment recording requires reconciliation",
                    ));
                }
                self.checkout.order_id = Some(order.id.as_ref().to_owned());
                self.checkout.payment_recording_state = PaymentRecordingState::Recording;
            }
        }
        if let Err(error) =
            crate::repositories::checkout::save(self.pool, self.checkout, self.token).await
        {
            self.checkpoint_error = Some(error);
            return Err(conflict(
                "Checkout completion checkpoint could not be saved",
            ));
        }
        if stage == services::checkout_completion::CompletionStage::ConvertDraft
            && self
                .checkout
                .reservation_expires_at
                .is_none_or(|expiry| expiry <= Utc::now())
        {
            return Err(recovery::manual(
                conflict("Checkout reservation expired before completion"),
                RecoveryCode::ReservationUnsafe,
            ));
        }
        self.attempted = true;
        Ok(())
    }
}

async fn save_completed_order(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    order_id: &ShopifyId,
    report_diagnostics: bool,
) -> Result<RecoveryOutcome, AdvanceError> {
    checkout.order_id = Some(order_id.as_ref().to_owned());
    checkout.completion_state = CompletionState::Uncertain;
    super::save(pool, checkout, token).await?;
    let order = match clients.shopify.checkout_order(order_id).await {
        Ok(Some(order)) => order,
        Ok(None) => return Ok(RecoveryOutcome::Retry(RecoveryCode::CompletionUncertain)),
        Err(error) => {
            return Ok(completion_failure(
                "Checkout order readback",
                &upstream(error),
                report_diagnostics,
            ));
        }
    };
    if checkout.completion_version == 2 {
        let draft = match purchase::read(clients, checkout).await {
            Ok(draft) => draft,
            Err(error) => {
                return Ok(completion_failure(
                    "Checkout final purchase readback",
                    &error,
                    report_diagnostics,
                ));
            }
        };
        let money = purchase::validate_purchase(checkout, &draft)?;
        let recording =
            services::checkout_completion::ExternalPaymentRecording::from_checkout(checkout)?;
        if let Err(error) = services::checkout_completion::validate_external_paid_order(
            &order,
            &purchase::exact(money.minor, &money.currency)?,
            &recording,
            if checkout.provider == "stripe" {
                "Stripe Card Payments"
            } else {
                "paypal"
            },
        ) {
            return Ok(completion_failure(
                "Checkout final external payment readback",
                &error,
                report_diagnostics,
            ));
        }
    }
    if let Some(outcome) = order_confirmation(checkout, &order) {
        return Ok(outcome);
    }
    checkout.completion_state = CompletionState::Completed;
    if checkout.completion_version == 2 {
        checkout.payment_recording_state = PaymentRecordingState::Recorded;
    }
    super::save(pool, checkout, token).await?;
    Ok(RecoveryOutcome::Observed(RecoveryCode::Observed))
}

fn order_confirmation(
    checkout: &Checkout,
    order: &crate::models::shopify::CompletionOrder,
) -> Option<RecoveryOutcome> {
    if order
        .name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        Some(RecoveryOutcome::Retry(RecoveryCode::CompletionUncertain))
    } else if !purchase::tags_match(checkout, &order.tags)
        || (matches!(checkout.correlation_version, 2 | 3)
            && !super::correlation::identity_matches(checkout, &order.custom_attributes))
        || (checkout.correlation_version == 1
            && checkout.completion_version == 2
            && !purchase::slug_matches(checkout, &order.custom_attributes))
    {
        Some(RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict))
    } else {
        None
    }
}

fn completion_mode(checkout: &Checkout, reservation_safe: bool) -> CompletionMode {
    if matches!(
        checkout.completion_state,
        CompletionState::Completing | CompletionState::Uncertain
    ) || !reservation_safe
    {
        CompletionMode::ReconcileOnly
    } else {
        CompletionMode::CompleteIfOpen
    }
}

fn completion_failure(
    operation: &'static str,
    error: &AppError,
    report_diagnostics: bool,
) -> RecoveryOutcome {
    if report_diagnostics {
        crate::error::log_background_error(operation, error);
    }
    recovery::classify(error)
}

fn metadata(checkout: &Checkout) -> BTreeMap<String, String> {
    BTreeMap::from([("checkout_id".into(), checkout.id.to_string())])
}
fn stripe_expected(
    checkout: &Checkout,
    money: &CheckoutMoney,
) -> Result<stripe::payment::ExpectedPayment, AppError> {
    Ok(stripe::payment::ExpectedPayment {
        session_id: checkout
            .provider_resource_id
            .clone()
            .ok_or_else(|| conflict("Stripe resource is missing"))?,
        account_id: checkout.provider_account_id.clone(),
        livemode: checkout
            .provider_environment
            .parse()
            .map_err(|_| conflict("Stripe environment is invalid"))?,
        amount: money.minor,
        currency: money.currency.clone(),
        metadata: metadata(checkout),
    })
}
fn paypal_expected(
    checkout: &Checkout,
    money: &CheckoutMoney,
) -> Result<paypal::payment::ExpectedPayment, AppError> {
    Ok(paypal::payment::ExpectedPayment {
        order_id: checkout
            .provider_resource_id
            .clone()
            .ok_or_else(|| conflict("PayPal resource is missing"))?,
        merchant_id: checkout.provider_account_id.clone(),
        api_origin: checkout.provider_environment.clone(),
        reference_id: checkout.id.to_string(),
        custom_id: checkout.id.to_string(),
        amount: paypal_minor(money)?,
        currency: money.currency.clone(),
    })
}

fn paypal_minor(money: &CheckoutMoney) -> Result<i64, AppError> {
    let currency = &money.currency;
    let canonical = purchase::exact(money.minor, currency)?.precision;
    let provider = paypal::payment::currency_scale(currency)
        .ok_or_else(|| conflict("Unsupported PayPal currency"))?;
    let places = usize::from(canonical)
        .checked_sub(provider)
        .ok_or_else(|| conflict("Invalid PayPal precision"))?;
    let scale = 10i64.pow(u32::try_from(places).map_err(|_| conflict("Invalid PayPal precision"))?);
    if money.minor % scale != 0 {
        return Err(conflict("PayPal amount requires whole currency units"));
    }
    Ok(money.minor / scale)
}

#[cfg(test)]
#[path = "../../tests/services/checkout/provider.rs"]
mod tests;
