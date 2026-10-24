use super::{CheckoutClients, conflict, purchase, unavailable};
use crate::{
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories::checkout as repo,
};
use crate::{
    error::upstream,
    models::{
        paypal::{
            self, CreateOrderRequest, ExperienceContext, OrderIntent, OrderStatus, Payee,
            PaymentMoney, PaymentSource, PaypalPaymentSource, PurchaseUnitRequest,
            ShippingPreference,
        },
        shopify::ShopifyId,
        stripe::{self, CheckoutMode, CreateCheckoutSession},
    },
    services::{
        checkout_completion::{self, CompletionMode, CompletionOutcome, PaymentContext},
        payments,
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
) -> Result<(), AppError> {
    if checkout.provider_resource_id.is_none() {
        if !reservation_safe {
            checkout.payment_state = if checkout.payment_state == PaymentState::NotStarted {
                PaymentState::Unpaid
            } else {
                PaymentState::Uncertain
            };
            return repo::save(pool, checkout, token).await;
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
            return repo::save(pool, checkout, token).await;
        }
        let initiation = if checkout.payment_state == PaymentState::NotStarted {
            InitiationMode::Fresh
        } else {
            InitiationMode::Recover
        };
        let fingerprint = initiation_fingerprint(checkout, money)?;
        if checkout
            .gateway_request_fingerprint
            .as_ref()
            .is_some_and(|stored| stored != &fingerprint)
        {
            return Err(conflict(
                "Checkout gateway parameters changed; reconciliation is required",
            ));
        }
        checkout.gateway_request_fingerprint = Some(fingerprint);
        checkout.payment_state = PaymentState::Creating;
        repo::save(pool, checkout, token).await?;
        let outcome = start_or_recover_payment(clients, checkout, money, initiation).await?;
        checkout.payment_state = match outcome {
            InitiationOutcome::RequiresAction => PaymentState::RequiresAction,
            InitiationOutcome::Reconcile => PaymentState::Processing,
        };
        checkout.success_url = None;
        checkout.cancel_url = None;
        repo::save(pool, checkout, token).await?;
        if outcome == InitiationOutcome::RequiresAction {
            return Ok(());
        }
    }
    if checkout.provider == "stripe" {
        reconcile_stripe_payment(pool, clients, checkout, money, token, reservation_safe).await
    } else {
        reconcile_paypal_payment(pool, clients, checkout, money, token, reservation_safe).await
    }
}

pub(super) async fn bind_authenticated_resource(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    resource: &str,
) -> Result<(), AppError> {
    if !matches!(
        checkout.payment_state,
        PaymentState::Creating | PaymentState::Uncertain
    ) || checkout.draft_state != DraftState::Reserved
        || checkout.provider_resource_id.is_some()
    {
        return Err(conflict("Payment initiation is not awaiting association"));
    }
    checkout.provider_resource_id = Some(resource.to_owned());
    if checkout.provider == "stripe" {
        if !resource.starts_with("cs_") {
            return Err(conflict("Stripe checkout resource is invalid"));
        }
        let client = clients.stripe.ok_or_else(unavailable)?;
        payments::stripe::verify_payment(client, &stripe_expected(checkout, money)?)
            .await
            .map_err(|error| upstream(error))?;
    } else {
        let client = clients.paypal.ok_or_else(unavailable)?;
        payments::paypal::verify_payment(client, &paypal_expected(checkout, money)?)
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
    repo::save(pool, checkout, token).await
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

async fn reconcile_stripe_payment(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    money: &CheckoutMoney,
    token: Uuid,
    reservation_safe: bool,
) -> Result<(), AppError> {
    let client = clients.stripe.ok_or_else(unavailable)?;
    let expected = stripe_expected(checkout, money)?;
    let outcome = payments::stripe::verify_payment(client, &expected)
        .await
        .map_err(|error| upstream(error))?;
    if checkout.payment_state == PaymentState::Paid
        && !matches!(&outcome, stripe::payment::PaymentOutcome::VerifiedPaid(_))
    {
        checkout.completion_state = CompletionState::Uncertain;
        return repo::save(pool, checkout, token).await;
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
            return repo::save(pool, checkout, token).await;
        }
        stripe::payment::PaymentOutcome::VerifiedPaid(paid) => {
            checkout.payment_state = PaymentState::Paid;
            repo::save(pool, checkout, token).await?;
            if paid.refunded || paid.amount_refunded != 0 || paid.disputed {
                checkout.completion_state = CompletionState::Uncertain;
                return repo::save(pool, checkout, token).await;
            }
        }
    }
    complete(
        pool,
        clients,
        checkout,
        token,
        reservation_safe,
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
) -> Result<(), AppError> {
    let client = clients.paypal.ok_or_else(unavailable)?;
    let expected = paypal_expected(checkout, money)?;
    let mut outcome = payments::paypal::verify_payment(client, &expected)
        .await
        .map_err(|error| upstream(error))?;
    if checkout.payment_state == PaymentState::Paid
        && !matches!(&outcome, paypal::payment::PaymentOutcome::VerifiedPaid(_))
    {
        checkout.completion_state = CompletionState::Uncertain;
        return repo::save(pool, checkout, token).await;
    }
    if matches!(outcome, paypal::payment::PaymentOutcome::Pending) {
        let order = client
            .get_order(&expected.order_id)
            .await
            .map_err(|error| upstream(error))?;
        if order.status == OrderStatus::Approved {
            if !reservation_safe {
                return pending(pool, checkout, token, false).await;
            }
            let current = purchase::read(clients, checkout).await?;
            if purchase::validate_reservation(checkout, &current).is_err()
                || checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
            {
                return pending(pool, checkout, token, false).await;
            }
            if Utc::now() >= checkout.created_at + Duration::hours(5) {
                checkout.payment_state = PaymentState::Uncertain;
                return repo::save(pool, checkout, token).await;
            }
            checkout.payment_state = PaymentState::Processing;
            repo::save(pool, checkout, token).await?;
            if checkout
                .reservation_expires_at
                .is_none_or(|expiry| expiry <= Utc::now())
            {
                return Ok(());
            }
            client
                .capture_order(&expected.order_id, &format!("capture-{}", checkout.id))
                .await
                .map_err(|error| upstream(error))?;
            outcome = payments::paypal::verify_payment(client, &expected)
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
            return repo::save(pool, checkout, token).await;
        }
        paypal::payment::PaymentOutcome::VerifiedPaid(_) => {
            checkout.payment_state = PaymentState::Paid;
            repo::save(pool, checkout, token).await?;
        }
    }
    complete(
        pool,
        clients,
        checkout,
        token,
        reservation_safe,
        PaymentContext::Paypal {
            client,
            expected: &expected,
        },
    )
    .await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InitiationMode {
    Fresh,
    Recover,
}

#[derive(PartialEq, Eq)]
enum InitiationOutcome {
    RequiresAction,
    Reconcile,
}

async fn pending(
    pool: &PgPool,
    checkout: &mut Checkout,
    token: Uuid,
    actionable: bool,
) -> Result<(), AppError> {
    checkout.payment_state = if actionable {
        PaymentState::RequiresAction
    } else {
        PaymentState::Processing
    };
    repo::save(pool, checkout, token).await
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
        initiate_paypal(clients, checkout, money, success, cancel).await
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
        return Err(conflict("Stripe account does not match checkout"));
    }
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
    let session = match mode {
        InitiationMode::Fresh => client.create_checkout_session(&request).await,
        InitiationMode::Recover => client.recover_checkout_session(&request).await,
    }
    .map_err(|error| upstream(error))?;
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
        return Err(conflict("Stripe checkout does not match purchase"));
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
    success: String,
    cancel: String,
) -> Result<InitiationOutcome, AppError> {
    let client = clients.paypal.ok_or_else(unavailable)?;
    if client.api_origin() != checkout.provider_environment {
        return Err(conflict("PayPal environment does not match checkout"));
    }
    let order = client
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
                            shipping_preference: Some(ShippingPreference::NoShipping),
                        },
                    },
                }),
                application_context: None,
            },
        )
        .await
        .map_err(|error| upstream(error))?;
    let expected = paypal::payment::ExpectedPayment {
        order_id: order.id.clone(),
        merchant_id: checkout.provider_account_id.clone(),
        api_origin: checkout.provider_environment.clone(),
        reference_id: checkout.id.to_string(),
        custom_id: checkout.id.to_string(),
        amount: paypal_minor(money)?,
        currency: money.currency.clone(),
    };
    payments::paypal::verify_payment(client, &expected)
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
    payment: PaymentContext<'_>,
) -> Result<(), AppError> {
    let draft = ShopifyId::new(
        checkout
            .draft_id
            .as_ref()
            .ok_or_else(|| conflict("Checkout draft is missing"))?,
    )
    .map_err(|error| upstream(error))?;
    let mode = if matches!(
        checkout.completion_state,
        CompletionState::Completing | CompletionState::Uncertain
    ) || !reservation_safe
    {
        CompletionMode::ReconcileOnly
    } else {
        CompletionMode::CompleteIfOpen
    };
    let current = match clients.shopify.checkout_draft(&draft).await {
        Ok(Some(current)) => current,
        Ok(None) => return Ok(()),
        Err(error) => {
            crate::error::log_background_error("Checkout draft readback", &upstream(error));
            return Ok(());
        }
    };
    if let Err(error) = purchase::validate_purchase(checkout, &current) {
        checkout.completion_state = CompletionState::Uncertain;
        repo::save(pool, checkout, token).await?;
        crate::error::log_background_error("Checkout purchase readback", &error);
        return Ok(());
    }
    let mut attempted = false;
    let result = {
        let checkout = &mut *checkout;
        let attempted = &mut attempted;
        checkout_completion::complete_paid_draft_with_before_completion(
            clients.shopify,
            &draft,
            payment,
            mode,
            move || async move {
                let current = purchase::read(clients, checkout).await?;
                purchase::validate_reservation(checkout, &current)?;
                if checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(conflict("Checkout reservation expired before completion"));
                }
                checkout.completion_state = CompletionState::Completing;
                repo::save(pool, checkout, token).await?;
                if checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(conflict("Checkout reservation expired before completion"));
                }
                *attempted = true;
                Ok(())
            },
        )
        .await
    };
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            checkout.completion_state = if attempted || mode == CompletionMode::ReconcileOnly {
                CompletionState::Uncertain
            } else {
                CompletionState::NotStarted
            };
            repo::save(pool, checkout, token).await?;
            crate::error::log_background_error("Checkout Shopify completion", &error);
            return Ok(());
        }
    };
    if let CompletionOutcome::Completed { order_id } = outcome {
        save_completed_order(pool, clients, checkout, token, &order_id).await?;
    } else {
        checkout.completion_state = if attempted || mode == CompletionMode::ReconcileOnly {
            CompletionState::Uncertain
        } else {
            CompletionState::NotStarted
        };
        repo::save(pool, checkout, token).await?;
    }
    Ok(())
}

async fn save_completed_order(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    order_id: &ShopifyId,
) -> Result<(), AppError> {
    checkout.order_id = Some(order_id.as_ref().to_owned());
    checkout.completion_state = CompletionState::Uncertain;
    repo::save(pool, checkout, token).await?;
    let order = match clients.shopify.checkout_order(order_id).await {
        Ok(Some(order)) => order,
        Ok(None) => return Ok(()),
        Err(error) => {
            crate::error::log_background_error("Checkout order readback", &upstream(error));
            return Ok(());
        }
    };
    let Some(_name) = order.name.filter(|name| !name.trim().is_empty()) else {
        return Ok(());
    };
    if !purchase::tags_match(checkout, &order.tags) {
        return Ok(());
    }
    checkout.completion_state = CompletionState::Completed;
    repo::save(pool, checkout, token).await?;
    Ok(())
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
