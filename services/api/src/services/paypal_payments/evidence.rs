use crate::{
    error::AppError,
    models::{
        paypal::{Capture, CaptureStatus, Order, OrderStatus, PaymentMoney},
        paypal_payments::{CaptureEvidence, PaypalPayment},
    },
    validations::paypal_payments::exact_money,
};
use axum::http::StatusCode;
use chrono::Utc;

pub(super) fn verify(
    payment: &PaypalPayment,
    order: &Order,
    capture: &Capture,
) -> Result<CaptureEvidence, AppError> {
    validate_capture_binding(payment, order, capture)?;
    let captures = order.purchase_units[0]
        .payments
        .as_ref()
        .map(|payments| payments.captures.as_slice())
        .unwrap_or_default();
    if order.status != OrderStatus::Completed || captures.len() != 1 {
        return Err(mismatch());
    }
    let embedded = &captures[0];
    for observed in [embedded, capture] {
        if observed.id.is_empty()
            || observed.id != capture.id
            || observed.status != CaptureStatus::Completed
            || observed.final_capture != Some(true)
            || !money_matches(payment, &observed.amount)
        {
            return Err(mismatch());
        }
    }
    Ok(CaptureEvidence {
        capture_id: capture.id.clone(),
        payment_id: payment.id,
        merchant_id: payment.merchant_id.clone(),
        environment: payment.environment.clone(),
        paypal_order_id: order.id.clone(),
        amount: payment.amount.clone(),
        currency: payment.currency.clone(),
        minor_units: payment.minor_units,
        provider_status: "COMPLETED".into(),
        verified_at: Utc::now(),
    })
}

pub(super) fn validate_capture_binding(
    payment: &PaypalPayment,
    order: &Order,
    capture: &Capture,
) -> Result<(), AppError> {
    validate_order(payment, order)?;
    if capture.id.is_empty() {
        return Err(mismatch());
    }
    let embedded = order.purchase_units[0]
        .payments
        .as_ref()
        .and_then(|payments| {
            payments
                .captures
                .iter()
                .find(|embedded| embedded.id == capture.id)
        })
        .ok_or_else(mismatch)?;
    for observed in [embedded, capture] {
        if observed
            .payee
            .as_ref()
            .is_some_and(|payee| payee.merchant_id != payment.merchant_id)
            || observed
                .custom_id
                .as_deref()
                .is_some_and(|id| id != payment.reference)
            || observed
                .supplementary_data
                .as_ref()
                .and_then(|data| data.related_ids.order_id.as_deref())
                .is_some_and(|id| Some(id) != payment.paypal_order_id.as_deref())
            || observed
                .supplementary_data
                .as_ref()
                .and_then(|data| data.related_ids.capture_id.as_deref())
                .is_some_and(|id| id != capture.id)
        {
            return Err(mismatch());
        }
    }
    Ok(())
}

pub(super) fn validate_order(payment: &PaypalPayment, order: &Order) -> Result<(), AppError> {
    if order.id.is_empty()
        || payment
            .paypal_order_id
            .as_ref()
            .is_some_and(|id| id != &order.id)
        || order.purchase_units.len() != 1
    {
        return Err(mismatch());
    }
    let unit = &order.purchase_units[0];
    if unit.reference_id.as_deref() != Some(payment.id.to_string().as_str())
        || unit.custom_id.as_deref() != Some(payment.reference.as_str())
        || !money_matches(payment, &unit.amount)
        || unit
            .payee
            .as_ref()
            .is_none_or(|payee| payee.merchant_id != payment.merchant_id)
    {
        return Err(mismatch());
    }
    Ok(())
}

pub(super) fn mismatch() -> AppError {
    AppError::new(
        StatusCode::BAD_GATEWAY,
        "PayPal payment evidence does not match",
    )
}

fn money_matches(payment: &PaypalPayment, amount: &PaymentMoney) -> bool {
    amount.currency_code == payment.currency
        && exact_money(&amount.value, &amount.currency_code)
            .is_ok_and(|money| money.minor_units == payment.minor_units)
}

#[cfg(test)]
#[path = "../../tests/services/paypal_payments/evidence.rs"]
mod tests;
