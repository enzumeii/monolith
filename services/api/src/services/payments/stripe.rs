use crate::{
    client::{ClientError, ClientErrorKind, stripe::StripeClient},
    models::stripe::{
        Account, Charge, ChargeStatus, CheckoutMode, CheckoutPaymentStatus, CheckoutSession,
        CheckoutStatus, PaymentIntent, PaymentIntentStatus, StripeReference,
        payment::{ExpectedPayment, PaymentOutcome, VerifiedPayment},
        valid_id as valid_stripe_id,
    },
};

/// Verifies the original successful payment. Refunds do not erase that success;
/// callers must inspect refunds separately before deciding what remains payable.
pub async fn verify_payment(
    client: &StripeClient,
    expected: &ExpectedPayment,
) -> Result<PaymentOutcome, ClientError> {
    validate_expected(expected)?;
    let account = client.get_account().await?;
    validate_account(&account, expected)?;
    let session = client.get_checkout_session(&expected.session_id).await?;
    let currency = expected.currency.to_ascii_lowercase();
    validate_session(&session, expected, &currency)?;
    match (&session.status, &session.payment_status) {
        (CheckoutStatus::Open, CheckoutPaymentStatus::Unpaid) => {
            return Ok(PaymentOutcome::RequiresAction);
        }
        (CheckoutStatus::Complete, CheckoutPaymentStatus::Unpaid) => {
            return Ok(PaymentOutcome::Pending);
        }
        (CheckoutStatus::Expired, CheckoutPaymentStatus::Unpaid) => {
            return Ok(PaymentOutcome::ExpiredUnpaid);
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
    validate_intent(&intent, intent_id, expected, &currency)?;
    let charge_id = intent
        .latest_charge
        .as_ref()
        .map(StripeReference::id)
        .filter(|id| valid_id(id, "ch_"))
        .ok_or_else(|| verification_error())?;
    let charge = client.get_charge(charge_id).await?;
    validate_charge(&charge, charge_id, &intent.id, expected, &currency)?;
    Ok(PaymentOutcome::VerifiedPaid(VerifiedPayment {
        session_id: session.id,
        payment_intent_id: intent.id,
        charge_id: charge.id,
        amount: expected.amount,
        currency,
        refunded: charge.payment_status.refunded,
        amount_refunded: charge.amount_refunded,
        disputed: charge.disputed,
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

fn validate_account(account: &Account, expected: &ExpectedPayment) -> Result<(), ClientError> {
    if account.id != expected.account_id || account.object != "account" {
        return Err(verification_error());
    }
    Ok(())
}

fn validate_session(
    session: &CheckoutSession,
    expected: &ExpectedPayment,
    currency: &str,
) -> Result<(), ClientError> {
    if session.id != expected.session_id
        || session.object != "checkout.session"
        || session.mode != CheckoutMode::Payment
        || session.livemode != expected.livemode
        || session.amount_total != Some(expected.amount)
        || session.currency.as_deref() != Some(currency)
        || session.metadata != expected.metadata
    {
        return Err(verification_error());
    }
    Ok(())
}

fn validate_intent(
    intent: &PaymentIntent,
    intent_id: &str,
    expected: &ExpectedPayment,
    currency: &str,
) -> Result<(), ClientError> {
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
    Ok(())
}

fn validate_charge(
    charge: &Charge,
    charge_id: &str,
    intent_id: &str,
    expected: &ExpectedPayment,
    currency: &str,
) -> Result<(), ClientError> {
    if charge.id != charge_id
        || charge.object.as_deref() != Some("charge")
        || charge.status != Some(ChargeStatus::Succeeded)
        || !charge.payment_status.paid
        || !charge.payment_status.captured
        || charge.payment_intent.as_deref() != Some(intent_id)
        || charge.amount != Some(expected.amount)
        || charge.amount_captured != expected.amount
        || charge.currency != currency
        || charge.livemode != Some(expected.livemode)
    {
        return Err(verification_error());
    }
    Ok(())
}

fn valid_id(id: &str, prefix: &str) -> bool {
    id.starts_with(prefix) && id.len() > prefix.len() && valid_stripe_id(id)
}

#[track_caller]
fn verification_error() -> ClientError {
    super::evidence_mismatch()
}

#[cfg(test)]
#[path = "../../tests/services/payments/stripe.rs"]
mod tests;
