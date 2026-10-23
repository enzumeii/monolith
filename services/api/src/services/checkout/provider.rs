use super::{CheckoutClients, conflict, purchase, unavailable};
use crate::{
    client::{paypal, stripe},
    error::upstream,
    models::{
        paypal::{
            CreateOrderRequest, ExperienceContext, OrderIntent, OrderStatus, Payee, PaymentMoney,
            PurchaseUnitRequest,
        },
        shopify::ShopifyId,
        stripe::{CheckoutMode, CreateCheckoutSession},
    },
    services::checkout_completion::{self, CompletionMode, CompletionOutcome, PaymentContext},
};
use crate::{
    config::CheckoutSettings,
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories::checkout as repo,
};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) async fn advance(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    if checkout.provider_resource_id.is_none() {
        if checkout
            .reservation_expires_at
            .is_none_or(|expiry| expiry <= Utc::now())
        {
            checkout.payment_state = PaymentState::Unpaid;
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
        checkout.payment_state = PaymentState::Creating;
        repo::save(pool, checkout, token).await?;
        initiate(settings, clients, checkout, initiation).await?;
        checkout.payment_state = PaymentState::RequiresAction;
        repo::save(pool, checkout, token).await?;
        return Ok(());
    }
    if checkout.provider == "stripe" {
        advance_stripe(pool, clients, checkout, token).await
    } else {
        advance_paypal(pool, clients, checkout, token).await
    }
}

pub(super) async fn bind_authenticated_resource(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    resource: &str,
) -> Result<(), AppError> {
    if checkout.payment_state != PaymentState::Creating
        || checkout.draft_state != DraftState::Reserved
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
        stripe::payment::verify_payment(client, &stripe_expected(checkout)?)
            .await
            .map_err(upstream)?;
        let session = client
            .get_checkout_session(resource)
            .await
            .map_err(upstream)?;
        checkout.redirect_url = session.url.as_deref().map(redirect).transpose()?;
    } else {
        let client = clients.paypal.ok_or_else(unavailable)?;
        paypal::payment::verify_payment(client, &paypal_expected(checkout)?)
            .await
            .map_err(upstream)?;
        let order = client.get_order(resource).await.map_err(upstream)?;
        checkout.redirect_url = order
            .links
            .iter()
            .find(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
            .map(|link| redirect(&link.href))
            .transpose()?;
    }
    checkout.payment_state = PaymentState::RequiresAction;
    repo::save(pool, checkout, token).await
}

async fn advance_stripe(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    let client = clients.stripe.ok_or_else(unavailable)?;
    let expected = stripe_expected(checkout)?;
    match stripe::payment::verify_payment(client, &expected)
        .await
        .map_err(upstream)?
    {
        stripe::payment::PaymentOutcome::Pending { .. } => {
            return pending(pool, checkout, token).await;
        }
        stripe::payment::PaymentOutcome::ExpiredUnpaid { .. } => {
            checkout.payment_state = PaymentState::Unpaid;
            return repo::save(pool, checkout, token).await;
        }
        stripe::payment::PaymentOutcome::VerifiedPaid(paid) => {
            checkout.payment_state = PaymentState::Paid;
            checkout.payment_reference = Some(paid.charge_id);
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
        PaymentContext::Stripe {
            client,
            expected: &expected,
        },
    )
    .await
}
async fn advance_paypal(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    let client = clients.paypal.ok_or_else(unavailable)?;
    let expected = paypal_expected(checkout)?;
    let mut outcome = paypal::payment::verify_payment(client, &expected)
        .await
        .map_err(upstream)?;
    if matches!(outcome, paypal::payment::PaymentOutcome::Pending { .. }) {
        let order = client
            .get_order(&expected.order_id)
            .await
            .map_err(upstream)?;
        if order.status == OrderStatus::Approved {
            if checkout
                .reservation_expires_at
                .is_none_or(|expiry| expiry <= Utc::now())
            {
                checkout.payment_state = PaymentState::Unpaid;
                return repo::save(pool, checkout, token).await;
            }
            if Utc::now() >= checkout.created_at + Duration::hours(5) {
                checkout.payment_state = PaymentState::Uncertain;
                return repo::save(pool, checkout, token).await;
            }
            checkout.payment_state = PaymentState::Processing;
            repo::save(pool, checkout, token).await?;
            client
                .capture_order(&expected.order_id, &checkout.capture_request_id)
                .await
                .map_err(upstream)?;
            outcome = paypal::payment::verify_payment(client, &expected)
                .await
                .map_err(upstream)?;
        }
    }
    match outcome {
        paypal::payment::PaymentOutcome::Pending { .. } => {
            return pending(pool, checkout, token).await;
        }
        paypal::payment::PaymentOutcome::TerminalUnpaid { .. } => {
            checkout.payment_state = PaymentState::Unpaid;
            return repo::save(pool, checkout, token).await;
        }
        paypal::payment::PaymentOutcome::VerifiedPaid(paid) => {
            checkout.payment_state = PaymentState::Paid;
            checkout.payment_reference = Some(paid.capture_id);
            repo::save(pool, checkout, token).await?;
        }
    }
    complete(
        pool,
        clients,
        checkout,
        token,
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

async fn pending(pool: &PgPool, checkout: &mut Checkout, token: Uuid) -> Result<(), AppError> {
    if checkout
        .reservation_expires_at
        .is_none_or(|expiry| expiry <= Utc::now())
    {
        checkout.payment_state = PaymentState::Unpaid;
        repo::save(pool, checkout, token).await?;
    }
    Ok(())
}

async fn initiate(
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    mode: InitiationMode,
) -> Result<(), AppError> {
    let success = return_url(settings, checkout, &settings.return_path_template);
    let cancel = return_url(settings, checkout, &settings.cancel_path_template);
    let money = &checkout.snapshot.money;
    if checkout.provider == "stripe" {
        let client = clients.stripe.ok_or_else(unavailable)?;
        let expiry = checkout
            .reservation_expires_at
            .ok_or_else(|| conflict("Reservation is missing"))?;
        if mode == InitiationMode::Fresh && expiry < Utc::now() + Duration::minutes(30) {
            return Err(conflict("Reservation is too short for Stripe checkout"));
        }
        let account = client.get_account().await.map_err(upstream)?;
        if account.id != checkout.provider_context.account_id || account.object != "account" {
            return Err(conflict("Stripe account does not match checkout"));
        }
        let request = CreateCheckoutSession {
            amount: money.minor,
            currency: money.currency.clone(),
            product_name: checkout.snapshot.convention_name.clone(),
            success_url: success,
            cancel_url: cancel,
            idempotency_key: checkout.create_request_id.clone(),
            expires_at: Some(expiry.timestamp()),
            metadata: metadata(checkout),
        };
        let session = match mode {
            InitiationMode::Fresh => client.create_checkout_session(&request).await,
            InitiationMode::Recover => client.recover_checkout_session(&request).await,
        }
        .map_err(upstream)?;
        if session.object != "checkout.session"
            || session.mode != CheckoutMode::Payment
            || session.livemode.to_string() != checkout.provider_context.environment
            || session.amount_total != Some(money.minor)
            || session.currency.as_deref() != Some(money.currency.to_ascii_lowercase().as_str())
            || session.metadata != metadata(checkout)
            || session
                .expires_at
                .is_none_or(|actual| actual > expiry.timestamp())
        {
            return Err(conflict("Stripe checkout does not match purchase"));
        }
        checkout.redirect_url =
            Some(redirect(session.url.as_deref().ok_or_else(|| {
                conflict("Stripe checkout URL is missing")
            })?)?);
        checkout.provider_resource_id = Some(session.id);
    } else {
        let client = clients.paypal.ok_or_else(unavailable)?;
        if client.api_origin() != checkout.provider_context.environment {
            return Err(conflict("PayPal environment does not match checkout"));
        }
        let order = client
            .create_order(
                &checkout.create_request_id,
                &CreateOrderRequest {
                    intent: OrderIntent::Capture,
                    purchase_units: vec![PurchaseUnitRequest {
                        reference_id: checkout.id.to_string(),
                        custom_id: checkout.id.to_string(),
                        amount: PaymentMoney {
                            currency_code: money.currency.clone(),
                            value: paypal_amount(checkout)?,
                        },
                        payee: Payee {
                            merchant_id: checkout.provider_context.account_id.clone(),
                        },
                    }],
                    payment_source: None,
                    application_context: Some(ExperienceContext {
                        return_url: success,
                        cancel_url: cancel,
                    }),
                },
            )
            .await
            .map_err(upstream)?;
        let expected = paypal::payment::ExpectedPayment {
            order_id: order.id.clone(),
            merchant_id: checkout.provider_context.account_id.clone(),
            api_origin: checkout.provider_context.environment.clone(),
            reference_id: checkout.id.to_string(),
            custom_id: checkout.id.to_string(),
            amount: paypal_minor(checkout)?,
            currency: money.currency.clone(),
        };
        paypal::payment::verify_payment(client, &expected)
            .await
            .map_err(upstream)?;
        let link = order
            .links
            .iter()
            .find(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
            .ok_or_else(|| conflict("PayPal approval URL is missing"))?;
        checkout.redirect_url = Some(redirect(&link.href)?);
        checkout.provider_resource_id = Some(order.id);
    }
    Ok(())
}

async fn complete(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
    payment: PaymentContext<'_>,
) -> Result<(), AppError> {
    let draft = ShopifyId::new(
        checkout
            .draft_id
            .as_ref()
            .ok_or_else(|| conflict("Checkout draft is missing"))?,
    )
    .map_err(upstream)?;
    let mode = if matches!(
        checkout.completion_state,
        CompletionState::Completing | CompletionState::Uncertain
    ) {
        CompletionMode::ReconcileOnly
    } else {
        CompletionMode::CompleteIfOpen
    };
    if checkout
        .reservation_expires_at
        .is_none_or(|expiry| expiry <= Utc::now())
        && mode == CompletionMode::CompleteIfOpen
    {
        checkout.completion_state = CompletionState::Uncertain;
        return repo::save(pool, checkout, token).await;
    }
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
                if checkout
                    .reservation_expires_at
                    .is_none_or(|expiry| expiry <= Utc::now())
                {
                    return Err(conflict("Checkout reservation expired before completion"));
                }
                checkout.completion_state = CompletionState::Completing;
                repo::save(pool, checkout, token).await?;
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
    let Some(name) = order.name.filter(|name| !name.trim().is_empty()) else {
        return Ok(());
    };
    if purchase::convention_tags(checkout)
        .iter()
        .any(|tag| !order.tags.contains(tag))
    {
        return Ok(());
    }
    checkout.order_name = Some(name);
    checkout.completion_state = CompletionState::Completed;
    repo::save(pool, checkout, token).await?;
    Ok(())
}

fn metadata(checkout: &Checkout) -> BTreeMap<String, String> {
    BTreeMap::from([("checkout_id".into(), checkout.id.to_string())])
}
fn stripe_expected(checkout: &Checkout) -> Result<stripe::payment::ExpectedPayment, AppError> {
    Ok(stripe::payment::ExpectedPayment {
        session_id: checkout
            .provider_resource_id
            .clone()
            .ok_or_else(|| conflict("Stripe resource is missing"))?,
        account_id: checkout.provider_context.account_id.clone(),
        livemode: checkout
            .provider_context
            .environment
            .parse()
            .map_err(|_| conflict("Stripe environment is invalid"))?,
        amount: checkout.snapshot.money.minor,
        currency: checkout.snapshot.money.currency.clone(),
        metadata: metadata(checkout),
    })
}
fn paypal_expected(checkout: &Checkout) -> Result<paypal::payment::ExpectedPayment, AppError> {
    Ok(paypal::payment::ExpectedPayment {
        order_id: checkout
            .provider_resource_id
            .clone()
            .ok_or_else(|| conflict("PayPal resource is missing"))?,
        merchant_id: checkout.provider_context.account_id.clone(),
        api_origin: checkout.provider_context.environment.clone(),
        reference_id: checkout.id.to_string(),
        custom_id: checkout.id.to_string(),
        amount: paypal_minor(checkout)?,
        currency: checkout.snapshot.money.currency.clone(),
    })
}
pub(super) fn paypal_amount(checkout: &Checkout) -> Result<String, AppError> {
    let minor = paypal_minor(checkout)?;
    let precision = paypal::payment::money::currency_scale(&checkout.snapshot.money.currency)
        .ok_or_else(|| conflict("Unsupported PayPal currency"))?;
    Ok(purchase::format_minor(
        minor,
        u8::try_from(precision).map_err(|_| conflict("Invalid PayPal precision"))?,
    ))
}

fn paypal_minor(checkout: &Checkout) -> Result<i64, AppError> {
    let currency = &checkout.snapshot.money.currency;
    let canonical = purchase::exact(checkout.snapshot.money.minor, currency)?.precision;
    let provider = paypal::payment::money::currency_scale(currency)
        .ok_or_else(|| conflict("Unsupported PayPal currency"))?;
    let places = usize::from(canonical)
        .checked_sub(provider)
        .ok_or_else(|| conflict("Invalid PayPal precision"))?;
    let scale = 10i64.pow(u32::try_from(places).map_err(|_| conflict("Invalid PayPal precision"))?);
    if checkout.snapshot.money.minor % scale != 0 {
        return Err(conflict("PayPal amount requires whole currency units"));
    }
    Ok(checkout.snapshot.money.minor / scale)
}
fn return_url(settings: &CheckoutSettings, checkout: &Checkout, template: &str) -> String {
    format!(
        "{}{}",
        settings.frontend_origin.trim_end_matches('/'),
        template
            .replace("{convention_slug}", &checkout.snapshot.convention_slug)
            .replace("{checkout_id}", &checkout.id.to_string())
    )
}
fn redirect(value: &str) -> Result<String, AppError> {
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
