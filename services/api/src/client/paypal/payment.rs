mod evidence;
mod money;

use crate::{
    client::{ClientError, ClientErrorKind, paypal::PaypalClient},
    models::paypal::{CaptureStatus, OrderStatus},
};
use reqwest::Url;

/// Expectations supplied by the trusted checkout owner, independently of provider evidence.
#[derive(Debug)]
pub struct ExpectedPayment {
    pub order_id: String,
    pub merchant_id: String,
    pub api_origin: String,
    pub reference_id: String,
    pub custom_id: String,
    pub amount: i64,
    pub currency: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaymentOutcome {
    Pending { order_id: String },
    TerminalUnpaid { order_id: String },
    VerifiedPaid(VerifiedPayment),
}

#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedPayment {
    pub order_id: String,
    pub capture_id: String,
    pub merchant_id: String,
    pub api_origin: String,
    pub reference_id: String,
    pub custom_id: String,
    pub amount: i64,
    pub currency: String,
}

/// Retrieves current provider evidence without capturing or mutating a payment.
/// Refunded, reversed and unknown capture states fail verification; this result
/// does not establish a refundable balance or authorize application fulfillment.
pub async fn verify_payment(
    client: &PaypalClient,
    expected: &ExpectedPayment,
) -> Result<PaymentOutcome, ClientError> {
    validate_expected(client, expected)?;
    let order = client.get_order(&expected.order_id).await?;
    evidence::validate_order(expected, &order)?;
    let captures = order.purchase_units[0]
        .payments
        .as_ref()
        .map(|payments| payments.captures.as_slice())
        .unwrap_or_default();
    if captures.is_empty() {
        return match order.status {
            OrderStatus::Created
            | OrderStatus::Saved
            | OrderStatus::Approved
            | OrderStatus::PayerActionRequired => {
                Ok(PaymentOutcome::Pending { order_id: order.id })
            }
            OrderStatus::Voided => Ok(PaymentOutcome::TerminalUnpaid { order_id: order.id }),
            _ => Err(evidence::mismatch()),
        };
    }
    if captures.len() != 1 || order.status != OrderStatus::Completed || !valid_id(&captures[0].id) {
        return Err(evidence::mismatch());
    }
    let embedded = &captures[0];
    let capture = client.get_capture(&embedded.id).await?;
    evidence::validate_capture_binding(expected, embedded, &capture)?;
    match capture.status {
        CaptureStatus::Pending => Ok(PaymentOutcome::Pending { order_id: order.id }),
        CaptureStatus::Denied | CaptureStatus::Declined | CaptureStatus::Failed => {
            Ok(PaymentOutcome::TerminalUnpaid { order_id: order.id })
        }
        CaptureStatus::Completed => {
            if embedded.final_capture != Some(true) || capture.final_capture != Some(true) {
                return Err(evidence::mismatch());
            }
            Ok(PaymentOutcome::VerifiedPaid(VerifiedPayment {
                order_id: order.id,
                capture_id: capture.id,
                merchant_id: expected.merchant_id.clone(),
                api_origin: client.api_origin(),
                reference_id: expected.reference_id.clone(),
                custom_id: expected.custom_id.clone(),
                amount: expected.amount,
                currency: expected.currency.clone(),
            }))
        }
        _ => Err(evidence::mismatch()),
    }
}

fn validate_expected(client: &PaypalClient, expected: &ExpectedPayment) -> Result<(), ClientError> {
    let origin = Url::parse(&expected.api_origin).ok();
    if !valid_id(&expected.order_id)
        || !valid_id(&expected.merchant_id)
        || !valid_correlation(&expected.reference_id)
        || !valid_correlation(&expected.custom_id)
        || expected.amount <= 0
        || money::currency_scale(&expected.currency).is_none()
        || origin.as_ref().is_none_or(|origin| {
            !matches!(origin.scheme(), "https" | "http")
                || origin.host_str().is_none()
                || !origin.username().is_empty()
                || origin.password().is_some()
                || !matches!(origin.path(), "" | "/")
                || origin.query().is_some()
                || origin.fragment().is_some()
                || origin.origin().ascii_serialization() != client.api_origin()
        })
    {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "Invalid expected payment context",
        )));
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && !id.chars().any(char::is_whitespace)
        && !id.chars().any(char::is_control)
}

fn valid_correlation(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 127 && !value.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "../../tests/client/paypal/payment.rs"]
mod tests;
