use super::{Gateway, ReconciliationOutcome, conflict, flag, reconcile};
use crate::{
    error::{AppError, upstream},
    models::{
        paypal::{VerifyWebhookSignatureRequest, WebhookEventType, WebhookVerificationStatus},
        paypal_payments::NewPaypalPaymentEvent,
    },
    repositories::paypal_payments as repository,
};
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub(crate) async fn receive_webhook(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    request: VerifyWebhookSignatureRequest,
) -> Result<(), AppError> {
    let event = &request.webhook_event;
    let EventIdentity {
        event_id,
        event_type,
        kind,
        capture_id,
        order_id,
        relevant,
    } = parse_event(event)?;
    let verified = gateway
        .client
        .verify_webhook_signature(&request)
        .await
        .map_err(|error| upstream(error))?;
    if verified.verification_status != WebhookVerificationStatus::Success {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "PayPal webhook signature rejected",
        ));
    }
    // Value's object map is ordered; serializing it canonicalizes insignificant JSON whitespace/key order.
    let digest: [u8; 32] = Sha256::digest(
        serde_json::to_vec(event).map_err(|error| AppError::from(anyhow::Error::new(error)))?,
    )
    .into();
    let new = NewPaypalPaymentEvent {
        merchant_id: gateway.policy.merchant_id.clone(),
        environment: gateway.environment.into(),
        event_id,
        payload_digest: digest,
        event_type,
        paypal_order_id: order_id.clone(),
        capture_id: capture_id.clone(),
        verified_at: Utc::now(),
    };
    let (saved, _) = repository::record_event(pool, &new).await?;
    if saved.payload_digest != digest {
        return Err(conflict("PayPal event identity has different payload"));
    }
    if saved.state != "PENDING" {
        return Ok(());
    }
    let identity = EventIdentity {
        event_id: new.event_id.clone(),
        event_type: new.event_type.clone(),
        kind,
        capture_id,
        order_id,
        relevant,
    };
    let payment_id = process_payment(pool, gateway, &identity).await?;
    let mut transaction = pool.begin().await?;
    let current = repository::lock_event(
        &mut transaction,
        &new.merchant_id,
        &new.environment,
        &new.event_id,
    )
    .await?;
    if current.payload_digest != digest {
        return Err(conflict("PayPal event identity has different payload"));
    }
    repository::finish_event(
        &mut transaction,
        &new.merchant_id,
        &new.environment,
        &new.event_id,
        payment_id,
        payment_id.is_none(),
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

struct EventIdentity {
    event_id: String,
    event_type: String,
    kind: WebhookEventType,
    capture_id: Option<String>,
    order_id: Option<String>,
    relevant: bool,
}

async fn process_payment(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    identity: &EventIdentity,
) -> Result<Option<uuid::Uuid>, AppError> {
    let order_id = if identity.relevant && identity.order_id.is_none() {
        if let Some(id) = identity.capture_id.as_deref() {
            let capture = gateway
                .client
                .get_capture(id)
                .await
                .map_err(|error| upstream(error))?;
            if capture.id != id {
                return Err(super::evidence::mismatch());
            }
            capture
                .supplementary_data
                .and_then(|data| data.related_ids.order_id)
        } else {
            None
        }
    } else {
        identity.order_id.clone()
    };
    let payment = if identity.relevant {
        if let Some(order_id) = order_id.as_deref() {
            repository::find_by_order(
                pool,
                &gateway.policy.merchant_id,
                gateway.environment,
                order_id,
            )
            .await?
        } else {
            None
        }
    } else {
        None
    };
    if let Some(payment) = &payment {
        gateway.check_binding(payment)?;
        if matches!(
            identity.kind,
            WebhookEventType::CaptureRefunded | WebhookEventType::CaptureReversed
        ) {
            review_capture(
                pool,
                gateway,
                payment,
                identity.capture_id.as_deref().ok_or_else(invalid_event)?,
            )
            .await?;
        } else {
            let outcome = reconcile(
                pool,
                gateway,
                payment,
                false,
                identity.capture_id.as_deref(),
            )
            .await?;
            require_event_evidence(&identity.kind, outcome)?;
        }
    }
    Ok(payment.map(|payment| payment.id))
}

fn require_event_evidence(
    kind: &WebhookEventType,
    outcome: ReconciliationOutcome,
) -> Result<(), AppError> {
    match outcome {
        ReconciliationOutcome::Busy => Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Payment reconciliation in progress",
        )),
        ReconciliationOutcome::ObservedUnpaid
            if matches!(
                kind,
                WebhookEventType::CaptureCompleted | WebhookEventType::OrderCompleted
            ) =>
        {
            Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "PayPal completed payment evidence is not available yet",
            ))
        }
        ReconciliationOutcome::ObservedUnpaid | ReconciliationOutcome::Verified => Ok(()),
    }
}

async fn review_capture(
    pool: &PgPool,
    gateway: &Gateway<'_>,
    payment: &crate::models::paypal_payments::PaypalPayment,
    id: &str,
) -> Result<(), AppError> {
    let order_id = payment
        .paypal_order_id
        .as_deref()
        .ok_or_else(invalid_event)?;
    let order = gateway
        .client
        .get_order(order_id)
        .await
        .map_err(|error| upstream(error))?;
    let capture = gateway
        .client
        .get_capture(id)
        .await
        .map_err(|error| upstream(error))?;
    if capture.id != id {
        return Err(super::evidence::mismatch());
    }
    super::evidence::validate_capture_binding(payment, &order, &capture)?;
    flag(pool, payment, "REFUND_OR_REVERSAL_REVIEW").await
}

fn parse_event(event: &Value) -> Result<EventIdentity, AppError> {
    let event_id = event
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| valid_identifier(value))
        .ok_or_else(invalid_event)?;
    let event_type = event
        .get("event_type")
        .and_then(Value::as_str)
        .filter(|value| valid_identifier(value))
        .ok_or_else(invalid_event)?;
    let resource = event
        .get("resource")
        .and_then(Value::as_object)
        .ok_or_else(invalid_event)?;
    let kind = WebhookEventType::from(event_type.to_owned());
    let refund = matches!(kind, WebhookEventType::CaptureRefunded);
    let refund_capture = resource
        .get("supplementary_data")
        .and_then(|value| value.get("related_ids"))
        .and_then(|value| value.get("capture_id"))
        .and_then(Value::as_str);
    let relevant =
        !matches!(kind, WebhookEventType::Other(_)) && (!refund || refund_capture.is_some());
    let capture_event = matches!(
        kind,
        WebhookEventType::CaptureCompleted
            | WebhookEventType::CapturePending
            | WebhookEventType::CaptureDenied
            | WebhookEventType::CaptureRefunded
            | WebhookEventType::CaptureReversed
    );
    let capture_id = if refund {
        refund_capture.map(str::to_owned)
    } else if capture_event {
        Some(
            resource
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| valid_identifier(id))
                .ok_or_else(invalid_event)?
                .to_owned(),
        )
    } else {
        None
    };
    let order_id = if matches!(
        kind,
        WebhookEventType::OrderApproved | WebhookEventType::OrderCompleted
    ) {
        Some(
            resource
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| valid_identifier(id))
                .ok_or_else(invalid_event)?
                .to_owned(),
        )
    } else {
        resource
            .get("supplementary_data")
            .and_then(|value| value.get("related_ids"))
            .and_then(|value| value.get("order_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    if order_id.as_deref().is_some_and(|id| !valid_identifier(id))
        || capture_id
            .as_deref()
            .is_some_and(|id| !valid_identifier(id))
    {
        return Err(invalid_event());
    }
    Ok(EventIdentity {
        event_id: event_id.into(),
        event_type: event_type.into(),
        kind,
        capture_id,
        order_id,
        relevant,
    })
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
}
fn invalid_event() -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, "Invalid PayPal webhook event")
}

#[cfg(test)]
#[path = "../../tests/services/paypal_payments/webhook.rs"]
mod tests;
