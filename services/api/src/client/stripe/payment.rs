use crate::{
    client::{
        ClientError, ClientErrorKind,
        stripe::{StripeClient, validate_id},
    },
    models::stripe::{
        ChargeStatus, CheckoutMode, CheckoutPaymentStatus, CheckoutStatus, PaymentIntentStatus,
        StripeReference,
    },
};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ExpectedPayment {
    pub session_id: String,
    pub account_id: String,
    pub livemode: bool,
    pub amount: i64,
    pub currency: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaymentOutcome {
    Pending { session_id: String },
    ExpiredUnpaid { session_id: String },
    VerifiedPaid(VerifiedPayment),
}

#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedPayment {
    pub account_id: String,
    pub session_id: String,
    pub payment_intent_id: String,
    pub charge_id: String,
    pub amount: i64,
    pub currency: String,
    pub livemode: bool,
}

/// Verifies the original successful payment. Refunds do not erase that success;
/// callers must inspect refunds separately before deciding what remains payable.
pub async fn verify_payment(
    client: &StripeClient,
    expected: &ExpectedPayment,
) -> Result<PaymentOutcome, ClientError> {
    validate_expected(expected)?;
    let account = client.get_account().await?;
    if account.id != expected.account_id || account.object != "account" {
        return Err(verification_error());
    }
    let session = client.get_checkout_session(&expected.session_id).await?;
    let currency = expected.currency.to_ascii_lowercase();
    if session.id != expected.session_id
        || session.object != "checkout.session"
        || session.mode != CheckoutMode::Payment
        || session.livemode != expected.livemode
        || session.amount_total != Some(expected.amount)
        || session.currency.as_deref() != Some(currency.as_str())
        || session.metadata != expected.metadata
    {
        return Err(verification_error());
    }
    match (&session.status, &session.payment_status) {
        (CheckoutStatus::Open | CheckoutStatus::Complete, CheckoutPaymentStatus::Unpaid) => {
            return Ok(PaymentOutcome::Pending {
                session_id: session.id,
            });
        }
        (CheckoutStatus::Expired, CheckoutPaymentStatus::Unpaid) => {
            return Ok(PaymentOutcome::ExpiredUnpaid {
                session_id: session.id,
            });
        }
        (CheckoutStatus::Complete, CheckoutPaymentStatus::Paid) => {}
        _ => return Err(verification_error()),
    }
    let intent_id = session
        .payment_intent
        .as_ref()
        .map(StripeReference::id)
        .filter(|id| valid_id(id, "pi_"))
        .ok_or_else(|| verification_error())?;
    let intent = client.get_payment_intent(intent_id).await?;
    if intent.id != intent_id
        || intent.object != "payment_intent"
        || intent.status != PaymentIntentStatus::Succeeded
        || intent.amount != expected.amount
        || intent.amount_received != expected.amount
        || intent.currency != currency
        || intent.livemode != expected.livemode
        || intent.metadata != expected.metadata
    {
        return Err(verification_error());
    }
    let charge_id = intent
        .latest_charge
        .as_ref()
        .map(StripeReference::id)
        .filter(|id| valid_id(id, "ch_"))
        .ok_or_else(|| verification_error())?;
    let charge = client.get_charge(charge_id).await?;
    if charge.id != charge_id
        || charge.object.as_deref() != Some("charge")
        || charge.status != Some(ChargeStatus::Succeeded)
        || !charge.payment_status.paid
        || !charge.payment_status.captured
        || charge.payment_intent.as_deref() != Some(intent.id.as_str())
        || charge.amount != Some(expected.amount)
        || charge.amount_captured != expected.amount
        || charge.currency != currency
        || charge.livemode != Some(expected.livemode)
    {
        return Err(verification_error());
    }
    Ok(PaymentOutcome::VerifiedPaid(VerifiedPayment {
        account_id: account.id,
        session_id: session.id,
        payment_intent_id: intent.id,
        charge_id: charge.id,
        amount: expected.amount,
        currency,
        livemode: expected.livemode,
    }))
}

fn validate_expected(expected: &ExpectedPayment) -> Result<(), ClientError> {
    if !valid_id(&expected.session_id, "cs_")
        || !valid_id(&expected.account_id, "acct_")
        || !(1..=99_999_999).contains(&expected.amount)
        || expected.currency.len() != 3
        || !expected
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
        || expected.metadata.is_empty()
        || expected.metadata.len() > 50
        || expected.metadata.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 40
                || key.contains(['[', ']'])
                || value.is_empty()
                || value.len() > 500
        })
    {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "Invalid expected payment context",
        )));
    }
    Ok(())
}

fn valid_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix) && id.len() > prefix.len() && validate_id(id).is_ok()
}

#[track_caller]
fn verification_error() -> ClientError {
    ClientError::new(ClientErrorKind::Protocol("Payment verification failed"))
}

#[cfg(test)]
#[path = "../../tests/client/stripe/payment.rs"]
mod tests;
