use crate::{
    client::{
        ClientErrorKind,
        paypal::{self, PaypalClient},
        shopify::{ShopifyClient, TagAction},
        stripe::{self, StripeClient},
    },
    error::{AppError, upstream},
    models::{
        checkout_completion::{ExactMoney, conflict, currency_precision},
        shopify::{
            CompletionDraft, CompletionDraftStatus, CompletionFinancialStatus, CompletionOrder,
            Money, ShopifyId,
        },
    },
};

/// The caller owns authorization, durable payment-to-draft association and serialization.
pub enum PaymentContext<'a> {
    Stripe {
        client: &'a StripeClient,
        expected: &'a stripe::payment::ExpectedPayment,
    },
    Paypal {
        client: &'a PaypalClient,
        expected: &'a paypal::payment::ExpectedPayment,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CompletionMode {
    CompleteIfOpen,
    /// Required after an uncertain completion attempt; never issues draftOrderComplete.
    ReconcileOnly,
}
#[derive(Debug)]
pub enum CompletionOutcome {
    Pending,
    Unpaid,
    Completed { order_id: ShopifyId },
}

/// Verify current provider evidence and reconcile current Shopify truth without a local ledger.
pub async fn complete_paid_draft(
    shopify: &ShopifyClient,
    draft_id: &ShopifyId,
    payment: PaymentContext<'_>,
    mode: CompletionMode,
) -> Result<CompletionOutcome, AppError> {
    complete_paid_draft_with_before_completion(shopify, draft_id, payment, mode, || async {
        Ok(())
    })
    .await
}

pub(crate) async fn complete_paid_draft_with_before_completion<Before, BeforeFuture>(
    shopify: &ShopifyClient,
    draft_id: &ShopifyId,
    payment: PaymentContext<'_>,
    mode: CompletionMode,
    before_completion: Before,
) -> Result<CompletionOutcome, AppError>
where
    Before: FnOnce() -> BeforeFuture + Send,
    BeforeFuture: std::future::Future<Output = Result<(), AppError>> + Send,
{
    draft_id
        .require_kind("DraftOrder")
        .map_err(|error| upstream(error))?;
    let (money, references) = match payment {
        PaymentContext::Stripe { client, expected } => {
            match stripe::payment::verify_payment(client, expected)
                .await
                .map_err(|error| upstream(error))?
            {
                stripe::payment::PaymentOutcome::Pending { .. } => {
                    return Ok(CompletionOutcome::Pending);
                }
                stripe::payment::PaymentOutcome::ExpiredUnpaid { .. } => {
                    return Ok(CompletionOutcome::Unpaid);
                }
                stripe::payment::PaymentOutcome::VerifiedPaid(paid) => {
                    if paid.refunded || paid.amount_refunded != 0 || paid.disputed {
                        return Err(conflict(
                            "Refunded or disputed payment cannot complete a draft",
                        ));
                    }
                    (
                        exact_money(paid.amount, &paid.currency)?,
                        vec![
                            reference("stripe:session", &paid.session_id)?,
                            reference("stripe:intent", &paid.payment_intent_id)?,
                            reference("stripe:charge", &paid.charge_id)?,
                        ],
                    )
                }
            }
        }
        PaymentContext::Paypal { client, expected } => {
            match paypal::payment::verify_payment(client, expected)
                .await
                .map_err(|error| upstream(error))?
            {
                paypal::payment::PaymentOutcome::Pending { .. } => {
                    return Ok(CompletionOutcome::Pending);
                }
                paypal::payment::PaymentOutcome::TerminalUnpaid { .. } => {
                    return Ok(CompletionOutcome::Unpaid);
                }
                paypal::payment::PaymentOutcome::VerifiedPaid(paid) => (
                    paypal_money(paid.amount, &paid.currency)?,
                    vec![
                        reference("paypal:order", &paid.order_id)?,
                        reference("paypal:capture", &paid.capture_id)?,
                    ],
                ),
            }
        }
    };
    complete_verified_draft(
        shopify,
        draft_id,
        &money,
        &references,
        mode,
        before_completion,
    )
    .await
}

async fn complete_verified_draft<Before, BeforeFuture>(
    shopify: &ShopifyClient,
    draft_id: &ShopifyId,
    money: &ExactMoney,
    references: &[String],
    mode: CompletionMode,
    before_completion: Before,
) -> Result<CompletionOutcome, AppError>
where
    Before: FnOnce() -> BeforeFuture + Send,
    BeforeFuture: std::future::Future<Output = Result<(), AppError>> + Send,
{
    let draft = read_draft(shopify, draft_id).await?;
    validate_money(money, &draft.total_price_set.presentment_money)?;
    let order_id = if let Some(order_id) = linked_order(&draft)? {
        order_id
    } else {
        validate_references(&draft.tags, references)?;
        if mode == CompletionMode::ReconcileOnly {
            return Err(reconciliation_required());
        }
        if !draft.ready || draft.payment_terms.is_some() {
            return Err(conflict(
                "Shopify draft is not eligible for paid completion",
            ));
        }
        before_completion().await?;
        match shopify.complete_checkout_draft(draft_id).await {
            Ok(order_id) => order_id,
            Err(error) => {
                if matches!(
                    error.kind(),
                    ClientErrorKind::Mutation { .. }
                        | ClientErrorKind::InvalidInput(_)
                        | ClientErrorKind::Protocol("completed draft identity mismatch")
                ) {
                    return Err(upstream(error));
                }
                let recovered = read_draft(shopify, draft_id)
                    .await
                    .map_err(|read_error| reconciliation_required().with_source(read_error))?;
                validate_money(money, &recovered.total_price_set.presentment_money)?;
                linked_order(&recovered)?
                    .ok_or_else(|| reconciliation_required().with_source(error))?
            }
        }
    };
    let order = read_order(shopify, &order_id, money).await?;
    validate_references(&order.tags, references)?;
    let missing: Vec<&str> = references
        .iter()
        .filter(|tag| !order.tags.contains(tag))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        let updated_id = shopify
            .edit_order_tags(order_id.as_ref(), &missing, TagAction::Add)
            .await
            .map_err(|error| upstream(error))?;
        if updated_id != order_id.as_ref() {
            return Err(conflict("Shopify tag mutation order identity mismatch"));
        }
        let confirmed = read_order(shopify, &order_id, money).await?;
        validate_references(&confirmed.tags, references)?;
        if references.iter().any(|tag| !confirmed.tags.contains(tag)) {
            return Err(reconciliation_required());
        }
    }
    Ok(CompletionOutcome::Completed { order_id })
}

fn exact_money(amount: i64, currency: &str) -> Result<ExactMoney, AppError> {
    let currency = currency.to_ascii_uppercase();
    let precision =
        currency_precision(&currency).ok_or_else(|| conflict("Unsupported payment currency"))?;
    ExactMoney::new(amount, &currency, precision)
}
fn paypal_money(amount: i64, currency: &str) -> Result<ExactMoney, AppError> {
    let mut money = exact_money(amount, currency)?;
    let provider_scale = paypal::payment::money::currency_scale(currency)
        .ok_or_else(|| conflict("Unsupported PayPal currency"))?;
    let provider_scale =
        u32::try_from(provider_scale).map_err(|_| conflict("Invalid PayPal currency precision"))?;
    let places = u32::from(money.precision)
        .checked_sub(provider_scale)
        .ok_or_else(|| conflict("Invalid PayPal currency precision"))?;
    money.minor = amount
        .checked_mul(10_i64.pow(places))
        .ok_or_else(|| conflict("Payment amount overflow"))?;
    Ok(money)
}
fn reference(kind: &str, id: &str) -> Result<String, AppError> {
    let tag = format!("payment:{kind}:{id}");
    if id.is_empty()
        || id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ':' | ','))
        || tag.chars().count() > 255
    {
        return Err(conflict("Invalid payment reference tag"));
    }
    Ok(tag)
}
fn validate_references(existing: &[String], expected: &[String]) -> Result<(), AppError> {
    if existing
        .iter()
        .any(|tag| tag.starts_with("payment:") && !expected.contains(tag))
    {
        return Err(conflict("Shopify order has conflicting payment references"));
    }
    Ok(())
}
async fn read_draft(shopify: &ShopifyClient, id: &ShopifyId) -> Result<CompletionDraft, AppError> {
    shopify
        .checkout_draft(id)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| conflict("Shopify draft is unavailable"))
}
fn linked_order(draft: &CompletionDraft) -> Result<Option<ShopifyId>, AppError> {
    match (&draft.status, &draft.order) {
        (CompletionDraftStatus::Completed, Some(order)) => {
            order
                .id
                .require_kind("Order")
                .map_err(|error| upstream(error))?;
            Ok(Some(order.id.clone()))
        }
        (CompletionDraftStatus::Open | CompletionDraftStatus::InvoiceSent, None) => Ok(None),
        _ => Err(conflict("Shopify draft and linked order state mismatch")),
    }
}
async fn read_order(
    shopify: &ShopifyClient,
    id: &ShopifyId,
    money: &ExactMoney,
) -> Result<CompletionOrder, AppError> {
    let order = shopify
        .checkout_order(id)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(reconciliation_required)?;
    if !matches!(
        order.display_financial_status,
        CompletionFinancialStatus::Paid
    ) || order.cancelled_at.is_some()
    {
        return Err(conflict("Shopify order is not paid or is cancelled"));
    }
    validate_money(money, &order.total_price_set.presentment_money)?;
    Ok(order)
}
fn validate_money(expected: &ExactMoney, actual: &Money) -> Result<(), AppError> {
    if actual.currency_code != expected.currency
        || expected.parse_amount(&actual.amount)? != expected.minor
    {
        return Err(conflict(
            "Shopify amount or currency does not match verified payment",
        ));
    }
    Ok(())
}
fn reconciliation_required() -> AppError {
    conflict(
        "Shopify completion requires reconciliation; use ReconcileOnly after an uncertain attempt",
    )
}

#[cfg(test)]
#[path = "../tests/services/checkout_completion.rs"]
mod tests;
