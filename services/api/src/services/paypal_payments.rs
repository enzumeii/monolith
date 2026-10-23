mod evidence;
mod webhook;

pub(crate) use webhook::receive_webhook;

use crate::{
    client::{ClientErrorKind, paypal::PaypalClient},
    config::{PaypalConfig, PaypalEnvironment, PaypalPaymentConfig},
    error::{AppError, upstream},
    models::{
        paypal::{
            CreateOrderRequest, ExperienceContext, Order, OrderIntent, OrderStatus, Payee,
            PaymentMoney, PaymentSource, PaypalPaymentSource, PurchaseUnitRequest,
        },
        paypal_payments::{CaptureEvidence, NewPaypalPayment, PaymentOperation, PaypalPayment},
    },
    repositories::paypal_payments as repository,
    validations::paypal_payments::{
        ValidatedPaymentInput, capability_matches, currency_scale, exact_money,
    },
};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use domain::dto::{PaypalPaymentResponse, PaypalPaymentStatus};
use reqwest::Url;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReconciliationOutcome {
    Busy,
    ObservedUnpaid,
    Verified,
}

pub(crate) struct Gateway<'a> {
    pub client: &'a PaypalClient,
    pub config: &'a PaypalConfig,
    pub policy: &'a PaypalPaymentConfig,
    pub environment: &'static str,
    pub origin: String,
}

impl<'a> Gateway<'a> {
    pub(crate) fn new(
        client: &'a PaypalClient,
        config: &'a PaypalConfig,
        insecure_local: bool,
    ) -> Result<Self, AppError> {
        let policy = config.payment.as_ref().ok_or_else(unavailable)?;
        let url = Url::parse(&config.base_url).map_err(|_| unavailable())?;
        let environment = match policy.environment {
            PaypalEnvironment::Sandbox => "sandbox",
            PaypalEnvironment::Live => "live",
        };
        let expected = match policy.environment {
            PaypalEnvironment::Sandbox => "https://api-m.sandbox.paypal.com",
            PaypalEnvironment::Live => "https://api-m.paypal.com",
        };
        let origin = url.origin().ascii_serialization();
        let controlled = insecure_local
            && policy.environment == PaypalEnvironment::Sandbox
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
        let destination = |value: &str| {
            Url::parse(value).is_ok_and(|url| {
                url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
                    && (url.scheme() == "https"
                        || (insecure_local
                            && url.scheme() == "http"
                            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))))
            })
        };
        if (!controlled && origin != expected)
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || policy.merchant_id.trim().is_empty()
            || policy.webhook_id.trim().is_empty()
            || policy.currencies.is_empty()
            || policy
                .currencies
                .iter()
                .any(|code| currency_scale(code).is_none())
            || !destination(&policy.return_url)
            || !destination(&policy.cancel_url)
        {
            return Err(unavailable());
        }
        Ok(Self {
            client,
            config,
            policy,
            environment,
            origin,
        })
    }

    pub(crate) fn check_binding(&self, payment: &PaypalPayment) -> Result<(), AppError> {
        if payment.merchant_id != self.policy.merchant_id
            || payment.environment != self.environment
            || payment.api_origin != self.origin
        {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "Payment provider configuration changed",
            ));
        }
        Ok(())
    }

    fn lease_until(&self) -> Result<DateTime<Utc>, AppError> {
        // Four authenticated operations may each refresh once and consume the bounded 429 budget.
        let seconds = self
            .config
            .timeout_secs
            .checked_mul(96)
            .and_then(|value| value.checked_add(600))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(unavailable)?;
        Utc::now()
            .checked_add_signed(Duration::seconds(seconds))
            .ok_or_else(unavailable)
    }
}

pub(crate) async fn create(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    input: ValidatedPaymentInput,
    digest: [u8; 32],
) -> Result<(StatusCode, PaypalPaymentResponse), AppError> {
    let new = new_payment(gateway, input, digest);
    let (payment, inserted) = repository::create_or_get(pool, &new).await?;
    authorize(&payment, &digest)?;
    if payment.reference != new.reference
        || payment.amount != new.amount
        || payment.currency != new.currency
    {
        return Err(conflict("Payment idempotency key has different inputs"));
    }
    gateway.check_binding(&payment)?;
    if payment.paypal_order_id.is_some() || payment.exception.is_some() {
        return Ok((StatusCode::OK, response(payment)));
    }
    if expired(payment.create_started_at) {
        flag(pool, &payment, "UNRESOLVED_CREATE").await?;
        return Ok((
            StatusCode::ACCEPTED,
            response(load(pool, payment.id).await?),
        ));
    }
    let token = Uuid::new_v4();
    let Some(claimed) = repository::claim_operation(
        pool,
        payment.id,
        PaymentOperation::Create,
        token,
        gateway.lease_until()?,
    )
    .await?
    else {
        return Ok((
            StatusCode::ACCEPTED,
            response(load(pool, payment.id).await?),
        ));
    };
    let payload = create_payload(gateway, &claimed);
    match gateway
        .client
        .create_order(&claimed.create_request_id.to_string(), &payload)
        .await
    {
        Ok(order) => {
            let validated = evidence::validate_order(&claimed, &order)
                .and_then(|()| approval_url(gateway, &order));
            match validated {
                Ok(url) => {
                    repository::save_order(pool, claimed.id, token, &order.id, Some(&url)).await?;
                }
                Err(error) => {
                    repository::finish_operation(
                        pool,
                        claimed.id,
                        token,
                        "EXCEPTION",
                        Some("INVALID_ORDER_EVIDENCE"),
                    )
                    .await?;
                    return Err(error);
                }
            }
        }
        Err(error) => {
            repository::finish_operation(pool, claimed.id, token, "CREATING", None).await?;
            if matches!(
                error.kind(),
                ClientErrorKind::Transport { .. }
                    | ClientErrorKind::Decode { .. }
                    | ClientErrorKind::RetryExhausted
                    | ClientErrorKind::Upstream { .. }
            ) {
                return Ok((
                    StatusCode::ACCEPTED,
                    response(load(pool, claimed.id).await?),
                ));
            }
            return Err(upstream(error));
        }
    }
    let saved = load(pool, claimed.id).await?;
    let status = if saved.paypal_order_id.is_none() {
        StatusCode::ACCEPTED
    } else if inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, response(saved)))
}

pub(crate) async fn get(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    id: Uuid,
    digest: [u8; 32],
) -> Result<PaypalPaymentResponse, AppError> {
    let payment = load(pool, id).await?;
    authorize(&payment, &digest)?;
    gateway.check_binding(&payment)?;
    if payment.paypal_order_id.is_some()
        && payment.exception.is_none()
        && payment.verified_at.is_none()
    {
        reconcile(pool, gateway, &payment, false, None).await?;
    }
    Ok(response(load(pool, id).await?))
}

pub(crate) async fn capture(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    id: Uuid,
    digest: [u8; 32],
) -> Result<(StatusCode, PaypalPaymentResponse), AppError> {
    let payment = load(pool, id).await?;
    authorize(&payment, &digest)?;
    gateway.check_binding(&payment)?;
    if payment.exception.is_some() {
        return Err(conflict("Payment requires review"));
    }
    if payment.verified_at.is_some() {
        return Ok((StatusCode::OK, response(payment)));
    }
    if payment.paypal_order_id.is_none() || payment.status == "DENIED" {
        return Err(conflict("Payment is not approved for capture"));
    }
    reconcile(pool, gateway, &payment, true, None).await?;
    let payment = load(pool, id).await?;
    let status = capture_response_status(&payment.status, payment.verified_at.is_some())?;
    Ok((status, response(payment)))
}

pub(crate) struct ExpectedPayment<'a> {
    pub reference: &'a str,
    pub amount: &'a str,
    pub currency: &'a str,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "ENZ-45 trusted completion integration seam")
)]
pub(crate) async fn verify_completed_payment(
    pool: &PgPool,
    client: &PaypalClient,
    config: &PaypalConfig,
    insecure_local: bool,
    payment_id: Uuid,
    expected: ExpectedPayment<'_>,
) -> Result<CaptureEvidence, AppError> {
    let gateway = Gateway::new(client, config, insecure_local)?;
    let payment = load(pool, payment_id).await?;
    gateway.check_binding(&payment)?;
    let money = exact_money(expected.amount, expected.currency)?;
    if payment.reference != expected.reference
        || payment.currency != money.currency
        || payment.minor_units != money.minor_units
        || payment.exception.is_some()
    {
        return Err(conflict("Payment does not match trusted expectations"));
    }
    if reconcile(pool, &gateway, &payment, false, None).await? != ReconciliationOutcome::Verified {
        return Err(conflict("Payment is not currently verified completed"));
    }
    let payment = load(pool, payment_id).await?;
    if payment.exception.is_some() || payment.verified_at.is_none() {
        return Err(conflict("Payment is not verified completed"));
    }
    let mut connection = pool.acquire().await?;
    let captures = repository::captures(&mut connection, payment_id).await?;
    if captures.len() != 1
        || Some(captures[0].capture_id.as_str()) != payment.verified_capture_id.as_deref()
    {
        return Err(conflict("Payment evidence requires review"));
    }
    Ok(captures.into_iter().next().expect("one verified capture"))
}

pub(super) async fn reconcile(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    payment: &PaypalPayment,
    allow_capture: bool,
    expected_capture: Option<&str>,
) -> Result<ReconciliationOutcome, AppError> {
    gateway.check_binding(payment)?;
    let token = Uuid::new_v4();
    let operation = if allow_capture {
        PaymentOperation::Capture
    } else {
        PaymentOperation::Reconcile
    };
    let Some(claimed) =
        repository::claim_operation(pool, payment.id, operation, token, gateway.lease_until()?)
            .await?
    else {
        return Ok(ReconciliationOutcome::Busy);
    };
    let result = reconcile_claimed(
        pool,
        gateway,
        &claimed,
        token,
        allow_capture,
        payment.capture_started_at.is_some(),
        expected_capture,
    )
    .await;
    if result.is_err() {
        repository::finish_operation(pool, payment.id, token, "PENDING", None).await?;
    }
    result
}

async fn reconcile_claimed(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    payment: &PaypalPayment,
    token: Uuid,
    allow_capture: bool,
    retry: bool,
    expected_capture: Option<&str>,
) -> Result<ReconciliationOutcome, AppError> {
    let order_id = payment
        .paypal_order_id
        .as_deref()
        .ok_or_else(|| conflict("Payment order is unresolved"))?;
    let mut order = gateway
        .client
        .get_order(order_id)
        .await
        .map_err(|error| upstream(error))?;
    if evidence::validate_order(payment, &order).is_err() {
        repository::finish_operation(
            pool,
            payment.id,
            token,
            "EXCEPTION",
            Some("INVALID_ORDER_EVIDENCE"),
        )
        .await?;
        return Err(evidence::mismatch());
    }
    if expected_capture.is_some_and(|id| {
        !order.purchase_units[0]
            .payments
            .as_ref()
            .is_some_and(|payments| payments.captures.iter().any(|capture| capture.id == id))
    }) {
        repository::finish_operation(
            pool,
            payment.id,
            token,
            "EXCEPTION",
            Some("WEBHOOK_CAPTURE_MISMATCH"),
        )
        .await?;
        return Err(evidence::mismatch());
    }
    if has_captures(&order) || order.status == OrderStatus::Completed {
        return observe(pool, gateway, payment, token, &order).await;
    }
    if matches!(order.status, OrderStatus::Voided | OrderStatus::Other(_)) {
        repository::finish_operation(pool, payment.id, token, "DENIED", None).await?;
        return if allow_capture {
            Err(conflict("Payment cannot be captured"))
        } else {
            Ok(ReconciliationOutcome::ObservedUnpaid)
        };
    }
    if !allow_capture {
        repository::finish_operation(pool, payment.id, token, "AWAITING_APPROVAL", None).await?;
        return Ok(ReconciliationOutcome::ObservedUnpaid);
    }
    if order.status != OrderStatus::Approved {
        repository::finish_operation(pool, payment.id, token, "AWAITING_APPROVAL", None).await?;
        return Err(conflict("Payment is not approved for capture"));
    }
    if retry && expired(payment.capture_started_at) {
        repository::finish_operation(
            pool,
            payment.id,
            token,
            "EXCEPTION",
            Some("UNRESOLVED_CAPTURE"),
        )
        .await?;
        return Err(conflict("Payment capture requires review"));
    }
    gateway
        .client
        .capture_order(order_id, &payment.capture_request_id.to_string())
        .await
        .map_err(|error| upstream(error))?;
    order = gateway
        .client
        .get_order(order_id)
        .await
        .map_err(|error| upstream(error))?;
    observe(pool, gateway, payment, token, &order).await
}

async fn observe(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    payment: &PaypalPayment,
    token: Uuid,
    order: &Order,
) -> Result<ReconciliationOutcome, AppError> {
    if evidence::validate_order(payment, order).is_err() {
        repository::finish_operation(
            pool,
            payment.id,
            token,
            "EXCEPTION",
            Some("INVALID_ORDER_EVIDENCE"),
        )
        .await?;
        return Err(evidence::mismatch());
    }
    let captures = order.purchase_units[0]
        .payments
        .as_ref()
        .map(|payments| payments.captures.as_slice())
        .unwrap_or_default();
    if captures.len() != 1 {
        repository::finish_operation(
            pool,
            payment.id,
            token,
            "EXCEPTION",
            Some("UNEXPECTED_CAPTURE_COUNT"),
        )
        .await?;
        return Err(evidence::mismatch());
    }
    let capture = gateway
        .client
        .get_capture(&captures[0].id)
        .await
        .map_err(|error| upstream(error))?;
    if capture.status == crate::models::paypal::CaptureStatus::Pending {
        repository::finish_operation(pool, payment.id, token, "PENDING", None).await?;
        return Ok(ReconciliationOutcome::ObservedUnpaid);
    }
    if matches!(
        capture.status,
        crate::models::paypal::CaptureStatus::Denied
            | crate::models::paypal::CaptureStatus::Declined
            | crate::models::paypal::CaptureStatus::Failed
    ) {
        repository::finish_operation(pool, payment.id, token, "DENIED", None).await?;
        return Ok(ReconciliationOutcome::ObservedUnpaid);
    }
    let capture = match evidence::verify(payment, order, &capture) {
        Ok(value) => value,
        Err(error) => {
            repository::finish_operation(
                pool,
                payment.id,
                token,
                "EXCEPTION",
                Some("INVALID_CAPTURE_EVIDENCE"),
            )
            .await?;
            return Err(error);
        }
    };
    persist_completed(pool, payment, token, &capture).await
}

async fn persist_completed(
    pool: &PgPool,
    payment: &PaypalPayment,
    token: Uuid,
    capture: &CaptureEvidence,
) -> Result<ReconciliationOutcome, AppError> {
    let mut transaction = pool.begin().await?;
    repository::lock_reference(&mut transaction, payment).await?;
    let current = repository::lock(&mut transaction, payment.id)
        .await?
        .ok_or_else(not_found)?;
    if current.lease_token != Some(token)
        || current.lease_until.is_none_or(|until| until <= Utc::now())
    {
        return Ok(ReconciliationOutcome::Busy);
    }
    let existing = repository::captures(&mut transaction, payment.id).await?;
    if !repository::save_capture(&mut transaction, capture).await? {
        repository::flag_exception(&mut transaction, payment.id, "CAPTURE_IDENTITY_CONFLICT")
            .await?;
        transaction.commit().await?;
        return Err(conflict("Payment evidence requires review"));
    }
    let multiple_captures = existing
        .iter()
        .any(|saved| saved.capture_id != capture.capture_id)
        || current
            .verified_capture_id
            .as_ref()
            .is_some_and(|id| id != &capture.capture_id);
    if multiple_captures {
        repository::flag_exception(&mut transaction, payment.id, "MULTIPLE_SUCCESSFUL_CAPTURES")
            .await?;
    } else if !repository::complete(
        &mut transaction,
        payment.id,
        Some(token),
        &capture.capture_id,
    )
    .await?
    {
        return Err(conflict("Payment verification changed"));
    }
    let duplicates = repository::duplicate_successful_reference(&mut transaction, payment).await?;
    if !duplicates.is_empty() {
        repository::flag_exception(
            &mut transaction,
            payment.id,
            "DUPLICATE_SUCCESSFUL_REFERENCE",
        )
        .await?;
        for id in duplicates {
            repository::flag_exception(&mut transaction, id, "DUPLICATE_SUCCESSFUL_REFERENCE")
                .await?;
        }
    }
    transaction.commit().await?;
    if multiple_captures {
        repository::finish_operation(pool, payment.id, token, "EXCEPTION", None).await?;
    }
    Ok(ReconciliationOutcome::Verified)
}

pub(super) async fn flag(
    pool: &PgPool,
    payment: &PaypalPayment,
    reason: &str,
) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    repository::lock_reference(&mut transaction, payment).await?;
    repository::lock(&mut transaction, payment.id).await?;
    repository::flag_exception(&mut transaction, payment.id, reason).await?;
    transaction.commit().await?;
    Ok(())
}

pub(super) async fn load(pool: &PgPool, id: Uuid) -> Result<PaypalPayment, AppError> {
    repository::find(pool, id).await?.ok_or_else(not_found)
}

fn capture_response_status(status: &str, verified: bool) -> Result<StatusCode, AppError> {
    if verified {
        Ok(StatusCode::OK)
    } else if status == "DENIED" {
        Err(conflict("Payment cannot be captured"))
    } else {
        Ok(StatusCode::ACCEPTED)
    }
}

fn new_payment(
    gateway: &Gateway<'_>,
    input: ValidatedPaymentInput,
    digest: [u8; 32],
) -> NewPaypalPayment {
    NewPaypalPayment {
        id: Uuid::new_v4(),
        idempotency_key: input.idempotency_key,
        capability_digest: digest,
        reference: input.reference,
        amount: input.amount,
        currency: input.currency,
        minor_units: input.minor_units,
        merchant_id: gateway.policy.merchant_id.clone(),
        environment: gateway.environment.into(),
        api_origin: gateway.origin.clone(),
        create_request_id: Uuid::new_v4(),
        capture_request_id: Uuid::new_v4(),
    }
}

fn create_payload(gateway: &Gateway<'_>, payment: &PaypalPayment) -> CreateOrderRequest {
    CreateOrderRequest {
        intent: OrderIntent::Capture,
        purchase_units: vec![PurchaseUnitRequest {
            reference_id: payment.id.to_string(),
            custom_id: payment.reference.clone(),
            amount: PaymentMoney {
                currency_code: payment.currency.clone(),
                value: payment.amount.clone(),
            },
            payee: Payee {
                merchant_id: payment.merchant_id.clone(),
            },
        }],
        payment_source: Some(PaymentSource {
            paypal: PaypalPaymentSource {
                experience_context: ExperienceContext {
                    return_url: gateway.policy.return_url.clone(),
                    cancel_url: gateway.policy.cancel_url.clone(),
                },
            },
        }),
        application_context: None,
    }
}

fn authorize(payment: &PaypalPayment, digest: &[u8]) -> Result<(), AppError> {
    if capability_matches(&payment.capability_digest, digest) {
        Ok(())
    } else {
        Err(not_found())
    }
}
fn expired(start: Option<DateTime<Utc>>) -> bool {
    start.is_some_and(|start| Utc::now().signed_duration_since(start) >= Duration::hours(6))
}
fn has_captures(order: &Order) -> bool {
    order.purchase_units.iter().any(|unit| {
        unit.payments
            .as_ref()
            .is_some_and(|payments| !payments.captures.is_empty())
    })
}
fn approval_url(gateway: &Gateway<'_>, order: &Order) -> Result<String, AppError> {
    let expected = if gateway.policy.environment == PaypalEnvironment::Sandbox {
        "www.sandbox.paypal.com"
    } else {
        "www.paypal.com"
    };
    let links = order
        .links
        .iter()
        .filter(|link| matches!(link.rel.as_str(), "approve" | "payer-action"))
        .collect::<Vec<_>>();
    if links.len() != 1 {
        return Err(evidence::mismatch());
    }
    let url = Url::parse(&links[0].href).map_err(|_| evidence::mismatch())?;
    if url.scheme() != "https"
        || url.host_str() != Some(expected)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || url.fragment().is_some()
    {
        return Err(evidence::mismatch());
    }
    Ok(url.into())
}
fn response(payment: PaypalPayment) -> PaypalPaymentResponse {
    let status = match payment.status.as_str() {
        "AWAITING_APPROVAL" => PaypalPaymentStatus::AwaitingApproval,
        "PENDING" => PaypalPaymentStatus::Pending,
        "COMPLETED" => PaypalPaymentStatus::Completed,
        "DENIED" => PaypalPaymentStatus::Denied,
        "EXCEPTION" => PaypalPaymentStatus::Exception,
        _ => PaypalPaymentStatus::Creating,
    };
    PaypalPaymentResponse {
        id: payment.id.to_string(),
        status,
        approval_url: payment.approval_url,
        paypal_order_id: payment.paypal_order_id,
        reference: payment.reference,
        amount: payment.amount,
        currency: payment.currency,
        capture_id: payment.verified_capture_id,
        exception: payment.exception,
    }
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "PayPal payments unavailable",
    )
}
pub(super) fn conflict(message: &'static str) -> AppError {
    AppError::new(StatusCode::CONFLICT, message)
}
fn not_found() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Payment not found")
}

#[cfg(test)]
#[path = "../tests/services/paypal_payments.rs"]
mod tests;
