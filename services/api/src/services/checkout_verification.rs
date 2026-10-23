use crate::{
    client::{paypal::PaypalClient, shopify::ShopifyClient, stripe::StripeClient},
    error::{AppError, upstream},
    models::{
        checkout_completion::{
            AliasKind, CompletionOutcome, CompletionRow, CompletionState, PaymentAlias,
            PaymentEnvironment, PaymentFact, PaymentState, Provider, VerifiedPayment, conflict,
        },
        paypal, stripe,
    },
    repositories::{checkout_completions as completions, checkout_verifications as repo},
    services::checkout_completion::apply_claimed_verified_payment,
};
use axum::http::StatusCode;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) fn cached(row: &CompletionRow) -> Result<Option<CompletionOutcome>, AppError> {
    if row.completion_state == CompletionState::Operator {
        return Err(conflict("Checkout requires operator review"));
    }
    if row.completion_state == CompletionState::Complete
        || matches!(
            row.payment_state,
            PaymentState::Unpaid | PaymentState::Expired
        )
        || row.next_verify_at.is_some_and(|next| next > Utc::now())
        || row.lease_until.is_some_and(|until| until > Utc::now())
    {
        return Ok(Some(CompletionOutcome::from(row)));
    }
    Ok(None)
}

pub(crate) async fn verify(
    pool: &PgPool,
    shopify: &ShopifyClient,
    stripe: Option<&StripeClient>,
    paypal: Option<&PaypalClient>,
    row: &CompletionRow,
) -> Result<CompletionOutcome, AppError> {
    let token = Uuid::new_v4();
    if !repo::claim(pool, row.id, token).await? {
        let current = completions::find(pool, row.id)
            .await?
            .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Checkout not found"))?;
        return Ok(cached(&current)?.unwrap_or_else(|| CompletionOutcome::from(&current)));
    }
    let result = async {
        let fact = if let Some(fact @ PaymentFact::Paid { .. }) = &row.trusted_payment {
            fact.clone()
        } else {
            match row.attempt.provider {
                Provider::Stripe => stripe_fact(stripe.ok_or_else(unavailable)?, row).await?,
                Provider::Paypal => paypal_fact(paypal.ok_or_else(unavailable)?, row).await?,
            }
        };
        let payment = VerifiedPayment {
            checkout_id: row.id,
            draft_id: row.snapshot.draft_id.clone(),
            attempt: row.attempt.clone(),
            event_id: format!("verification-{token}"),
            state: fact,
        };
        apply_claimed_verified_payment(pool, shopify, &payment, token).await
    }
    .await;
    if result.is_err() {
        repo::release(pool, row.id, token).await?;
    }
    result
}

async fn stripe_fact(client: &StripeClient, row: &CompletionRow) -> Result<PaymentFact, AppError> {
    use stripe::{CheckoutMode, CheckoutPaymentStatus, CheckoutStatus, PaymentIntentStatus};
    let account = client
        .get_account()
        .await
        .map_err(|error| upstream(error))?;
    required(&account.id)?;
    association(account.id == row.attempt.account)?;
    let session = client
        .get_checkout_session(&row.attempt.reference)
        .await
        .map_err(|error| upstream(error))?;
    association(session.id == row.attempt.reference && session.livemode == live(row))?;
    association(session.client_reference_id.as_deref() == Some(row.id.to_string().as_str()))?;
    stripe_binding(row, &session.metadata)?;
    match session.mode {
        CheckoutMode::Payment => {}
        CheckoutMode::Other(_) => return Err(invalid()),
        _ => return Err(conflict("Payment association mismatch")),
    }
    if matches!(session.status, CheckoutStatus::Other(_))
        || matches!(session.payment_status, CheckoutPaymentStatus::Other(_))
    {
        return Err(invalid());
    }
    stripe_money(
        row,
        session.amount_total.ok_or_else(invalid)?,
        session.currency.as_deref().ok_or_else(invalid)?,
    )?;
    if session.payment_status != CheckoutPaymentStatus::Paid {
        return match (session.status, session.payment_status) {
            (CheckoutStatus::Expired, CheckoutPaymentStatus::Unpaid) => Ok(PaymentFact::Expired),
            (_, CheckoutPaymentStatus::NoPaymentRequired) => {
                Err(conflict("Payment amount or currency mismatch"))
            }
            _ => Ok(PaymentFact::Pending),
        };
    }
    if session.status != CheckoutStatus::Complete {
        return Err(invalid());
    }
    let intent_id = session
        .payment_intent
        .as_ref()
        .map(stripe::StripeReference::id)
        .ok_or_else(invalid)?;
    required(intent_id)?;
    let intent = client
        .get_payment_intent(intent_id)
        .await
        .map_err(|error| upstream(error))?;
    association(intent.id == intent_id && intent.livemode == live(row))?;
    stripe_binding(row, &intent.metadata)?;
    stripe_money(row, intent.amount, &intent.currency)?;
    match intent.status {
        PaymentIntentStatus::Succeeded => {}
        PaymentIntentStatus::Other(_) => return Err(invalid()),
        _ => return Ok(PaymentFact::Pending),
    }
    stripe_money(row, intent.amount_received, &intent.currency)?;
    let charge_id = intent
        .latest_charge
        .as_ref()
        .map(stripe::StripeReference::id)
        .ok_or_else(invalid)?;
    required(charge_id)?;
    let charge = client
        .get_charge(charge_id)
        .await
        .map_err(|error| upstream(error))?;
    let charge_live = charge.livemode.ok_or_else(invalid)?;
    association(
        charge.id == charge_id
            && charge_live == live(row)
            && charge.payment_intent.as_deref() == Some(intent_id),
    )?;
    stripe_money(row, charge.amount_captured, &charge.currency)?;
    if charge.disputed || charge.payment_status.refunded || charge.amount_refunded != 0 {
        return Err(conflict("Payment requires operator review"));
    }
    if !charge.payment_status.paid || !charge.payment_status.captured {
        return Ok(PaymentFact::Pending);
    }
    Ok(PaymentFact::Paid {
        money: row.snapshot.accepted.clone(),
        aliases: vec![
            alias(AliasKind::StripeSession, &session.id),
            alias(AliasKind::StripePaymentIntent, &intent.id),
            alias(AliasKind::StripeCharge, &charge.id),
        ],
    })
}
async fn paypal_fact(client: &PaypalClient, row: &CompletionRow) -> Result<PaymentFact, AppError> {
    use paypal::{CaptureStatus, EndpointEnvironment, OrderIntent, OrderStatus};
    let environment = client.endpoint_environment().ok_or_else(unavailable)?;
    association(matches!(
        (environment, row.attempt.environment),
        (EndpointEnvironment::Sandbox, PaymentEnvironment::Test)
            | (EndpointEnvironment::Live, PaymentEnvironment::Live)
    ))?;
    let order = client
        .get_order(&row.attempt.reference)
        .await
        .map_err(|error| upstream(error))?;
    association(order.id == row.attempt.reference)?;
    match order.intent {
        Some(OrderIntent::Capture) => {}
        Some(OrderIntent::Other(_)) | None => return Err(invalid()),
        Some(OrderIntent::Authorize) => return Err(conflict("Payment association mismatch")),
    }
    if order.purchase_units.len() != 1 {
        return Err(invalid());
    }
    let unit = &order.purchase_units[0];
    association(
        unit.custom_id.as_deref() == Some(row.id.to_string().as_str())
            && unit.reference_id.as_deref() == Some(row.attempt.attempt_id.as_str()),
    )?;
    let account = unit
        .payee
        .as_ref()
        .map(|payee| payee.merchant_id.as_str())
        .ok_or_else(invalid)?;
    required(account)?;
    association(account == row.attempt.account)?;
    paypal_money(row, &unit.amount)?;
    match order.status {
        OrderStatus::Voided => return Ok(PaymentFact::Expired),
        OrderStatus::Completed => {}
        OrderStatus::Other(_) => return Err(invalid()),
        _ => return Ok(PaymentFact::Pending),
    }
    let captures = &unit.payments.as_ref().ok_or_else(invalid)?.captures;
    if captures.len() != 1 {
        return Err(invalid());
    }
    let capture = &captures[0];
    required(&capture.id)?;
    if capture
        .payee
        .as_ref()
        .map(|p| p.merchant_id.as_str())
        .is_some_and(|merchant| merchant != row.attempt.account)
        || capture
            .custom_id
            .as_deref()
            .is_some_and(|id| id != row.id.to_string())
        || capture
            .supplementary_data
            .as_ref()
            .and_then(|d| d.related_ids.order_id.as_deref())
            .is_some_and(|id| id != order.id)
    {
        return Err(conflict("Payment association mismatch"));
    }
    paypal_money(row, &capture.amount)?;
    match capture.status {
        CaptureStatus::Completed => {}
        CaptureStatus::Pending => return Ok(PaymentFact::Pending),
        CaptureStatus::Denied | CaptureStatus::Other(_) => return Err(invalid()),
        _ => return Err(conflict("Payment requires operator review")),
    }
    if capture.final_capture != Some(true) {
        return Err(invalid());
    }
    Ok(PaymentFact::Paid {
        money: row.snapshot.accepted.clone(),
        aliases: vec![
            alias(AliasKind::PaypalOrder, &order.id),
            alias(AliasKind::PaypalCapture, &capture.id),
        ],
    })
}
fn stripe_binding(
    row: &CompletionRow,
    metadata: &std::collections::BTreeMap<String, String>,
) -> Result<(), AppError> {
    association(
        metadata.get("checkout_id") == Some(&row.id.to_string())
            && metadata.get("attempt_id") == Some(&row.attempt.attempt_id),
    )
}
fn stripe_money(row: &CompletionRow, amount: i64, currency: &str) -> Result<(), AppError> {
    let expected = &row.snapshot.accepted;
    expected.validate()?;
    if expected.precision > 2 {
        return Err(conflict("Unsupported provider currency precision"));
    }
    association(currency.eq_ignore_ascii_case(&expected.currency) && amount == expected.minor)
}
fn paypal_money(row: &CompletionRow, money: &paypal::PaymentMoney) -> Result<(), AppError> {
    let expected = &row.snapshot.accepted;
    if crate::models::CurrencyCode::from(money.currency_code.clone()).as_str() != expected.currency
    {
        return Err(conflict("Payment amount or currency mismatch"));
    }
    let amount = expected.parse_amount(&money.value).map_err(|_| invalid())?;
    if amount != expected.minor {
        return Err(conflict("Payment amount or currency mismatch"));
    }
    Ok(())
}
fn alias(kind: AliasKind, value: &str) -> PaymentAlias {
    PaymentAlias {
        kind,
        value: value.into(),
    }
}
fn live(row: &CompletionRow) -> bool {
    row.attempt.environment == PaymentEnvironment::Live
}
fn association(matches: bool) -> Result<(), AppError> {
    if matches {
        Ok(())
    } else {
        Err(conflict("Payment association or amount mismatch"))
    }
}
fn required(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn invalid() -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, "Invalid upstream payment response")
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Payment provider unavailable",
    )
}
