use super::{checkout::correlation, payments};
use crate::{
    client::{
        ClientErrorKind,
        paypal::PaypalClient,
        shopify::{ShopifyClient, TagAction},
        stripe::StripeClient,
    },
    error::{AppError, upstream},
    models::{
        checkout_completion::{
            CompletionEvidenceConflict, ExactMoney, conflict, currency_precision, evidence_conflict,
        },
        paypal,
        shopify::{
            CompletionDraft, CompletionDraftStatus, CompletionFinancialStatus, CompletionOrder,
            CustomAttribute, Money, ShopifyId,
        },
        stripe,
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

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
    let (money, references) = match verified_payment(payment).await? {
        VerifiedPayment::Paid {
            money, references, ..
        } => (money, references),
        VerifiedPayment::Waiting(outcome) => return Ok(outcome),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionStage {
    ConvertDraft,
    RecordExternalPayment,
}
pub(crate) trait CompletionProgress {
    fn before_mutation(
        &mut self,
        stage: CompletionStage,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;
}
pub(crate) struct ExternalPaymentRecording<'a> {
    pub state: crate::models::checkout::PaymentRecordingState,
    pub required_tag_hashes: &'a [Vec<u8>],
    pub convention_slug: &'a str,
    pub identity: Option<correlation::Identity>,
    pub provider_resource_id: Option<&'a str>,
}
impl<'a> ExternalPaymentRecording<'a> {
    pub(crate) fn from_checkout(
        checkout: &'a crate::models::checkout::Checkout,
    ) -> Result<Self, AppError> {
        let identity = match checkout.correlation_version {
            correlation::LEGACY_VERSION => None,
            correlation::METADATA_VERSION => Some(correlation::Identity::from_checkout(checkout)?),
            _ => {
                return Err(evidence_conflict(
                    "Unsupported checkout correlation version",
                ));
            }
        };
        Ok(Self {
            state: checkout.payment_recording_state,
            required_tag_hashes: &checkout.required_tag_hashes,
            convention_slug: checkout
                .convention_slug
                .as_deref()
                .ok_or_else(|| evidence_conflict("Checkout convention binding is missing"))?,
            identity,
            provider_resource_id: checkout.provider_resource_id.as_deref(),
        })
    }
}

pub(crate) async fn complete_external_paid_draft(
    shopify: &ShopifyClient,
    draft_id: &ShopifyId,
    payment: PaymentContext<'_>,
    mode: CompletionMode,
    recording: ExternalPaymentRecording<'_>,
    progress: &mut impl CompletionProgress,
) -> Result<CompletionOutcome, AppError> {
    use crate::models::checkout::PaymentRecordingState;
    draft_id
        .require_kind("DraftOrder")
        .map_err(|error| upstream(error))?;
    let (money, references, attributes, method) = match verified_payment(payment).await? {
        VerifiedPayment::Waiting(outcome) => return Ok(outcome),
        VerifiedPayment::Paid {
            money,
            references,
            attributes,
            method,
        } => (money, references, attributes, method),
    };
    if let Some(identity) = &recording.identity {
        validate_final_payment_attributes(&attributes, identity, recording.provider_resource_id)?;
    }
    let draft = read_draft(shopify, draft_id).await?;
    validate_money(&money, &draft.total_price_set.presentment_money)?;
    validate_external_binding(&recording, &draft.tags, &draft.custom_attributes)?;
    validate_external_references(
        &recording,
        &draft.tags,
        &draft.custom_attributes,
        &references,
        &attributes,
    )?;
    let order_id = convert_or_reconcile_external_draft(
        shopify, draft_id, &draft, &money, mode, &recording, progress,
    )
    .await?;
    let mut order = read_external_order(shopify, &order_id, &money, &recording).await?;
    validate_external_references(
        &recording,
        &order.tags,
        &order.custom_attributes,
        &references,
        &attributes,
    )?;
    if !matches!(
        order.display_financial_status,
        CompletionFinancialStatus::Paid
    ) {
        if recording.state != PaymentRecordingState::NotStarted {
            return Err(reconciliation_required());
        }
        validate_recordable_order(&order, &money)?;
        progress
            .before_mutation(CompletionStage::RecordExternalPayment)
            .await?;
        let response = shopify
            .record_checkout_payment(&order_id, recording.identity.is_none().then_some(method))
            .await;
        order = read_external_order(shopify, &order_id, &money, &recording)
            .await
            .map_err(|read| reconciliation_required().with_source(read))?;
        if let Err(error) = response {
            if std::error::Error::source(&error).is_some_and(|source| {
                source.is::<crate::client::shopify::CheckoutIdentityMismatch>()
            }) {
                return Err(upstream(error));
            }
            if !matches!(
                order.display_financial_status,
                CompletionFinancialStatus::Paid
            ) {
                return Err(reconciliation_required().with_source(error));
            }
        }
    }
    validate_external_sale(&order, &money, &recording, method)?;
    order = save_verified_references(shopify, order, &money, &recording, &references, &attributes)
        .await?;
    validate_external_paid_order(&order, &money, &recording, method)?;
    Ok(CompletionOutcome::Completed { order_id })
}

pub(crate) fn validate_external_paid_order(
    order: &CompletionOrder,
    money: &ExactMoney,
    recording: &ExternalPaymentRecording<'_>,
    method: &str,
) -> Result<(), AppError> {
    validate_external_sale(order, money, recording, method)?;
    if let Some(identity) = &recording.identity {
        validate_final_payment_attributes(
            &order.custom_attributes,
            identity,
            recording.provider_resource_id,
        )?;
    }
    Ok(())
}
pub(crate) fn reference(kind: &str, id: &str) -> Result<String, AppError> {
    let tag = format!("payment:{kind}:{id}");
    if id.is_empty()
        || id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ':' | ','))
        || tag.chars().count() > 255
    {
        return Err(conflict("Invalid payment reference tag"));
    }
    if tag.chars().count() > 40 {
        let digest = Sha256::digest(tag.as_bytes());
        return Ok(format!("payment:{}", URL_SAFE_NO_PAD.encode(&digest[..24])));
    }
    Ok(tag)
}
fn validate_external_sale(
    order: &CompletionOrder,
    money: &ExactMoney,
    recording: &ExternalPaymentRecording<'_>,
    method: &str,
) -> Result<(), AppError> {
    validate_external_binding(recording, &order.tags, &order.custom_attributes)?;
    if order.cancelled_at.is_some() {
        return Err(evidence_conflict("Shopify order was cancelled"));
    }
    validate_money(money, &order.total_price_set.presentment_money)?;
    if recording.identity.is_some()
        && (external_transactions(order)?.len() >= 100
            || external_transactions(order)?
                .iter()
                .any(|transaction| !transaction.kind.eq_ignore_ascii_case("SALE")))
    {
        return Err(evidence_conflict(
            "Shopify order has conflicting payment transactions",
        ));
    }
    validate_recorded_order(
        order,
        money,
        if recording.identity.is_some() {
            "manual"
        } else {
            method
        },
    )
}
async fn save_verified_references(
    shopify: &ShopifyClient,
    mut order: CompletionOrder,
    money: &ExactMoney,
    recording: &ExternalPaymentRecording<'_>,
    references: &[String],
    attributes: &[CustomAttribute],
) -> Result<CompletionOrder, AppError> {
    let order_id = order.id.clone();
    if recording.identity.is_some() {
        validate_payment_attributes(&order.custom_attributes, attributes, false)?;
        let missing: Vec<_> = attributes
            .iter()
            .filter(|expected| {
                !order
                    .custom_attributes
                    .iter()
                    .any(|actual| actual.key == expected.key && actual.value == expected.value)
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            let mut merged = order.custom_attributes.clone();
            merged.extend(missing);
            let response = shopify
                .update_order_custom_attributes(&order_id, &merged)
                .await;
            order = read_external_order(shopify, &order_id, money, recording)
                .await
                .map_err(|read| reconciliation_required().with_source(read))?;
            if let Err(error) = response {
                if std::error::Error::source(&error).is_some_and(|source| {
                    source.is::<crate::client::shopify::CheckoutIdentityMismatch>()
                }) {
                    return Err(upstream(error));
                }
                if let Err(read) =
                    validate_payment_attributes(&order.custom_attributes, attributes, true)
                {
                    if read.find_source::<CompletionEvidenceConflict>().is_some() {
                        return Err(read);
                    }
                    return Err(read.with_source(error));
                }
            }
        }
        validate_payment_attributes(&order.custom_attributes, attributes, true)?;
    } else {
        validate_references(&order.tags, references)?;
        let missing: Vec<&str> = references
            .iter()
            .filter(|tag| !order.tags.contains(tag))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            let id = shopify
                .edit_order_tags(order_id.as_ref(), &missing, TagAction::Add)
                .await
                .map_err(|error| upstream(error))?;
            if id != order_id.as_ref() {
                return Err(evidence_conflict(
                    "Shopify tag mutation order identity mismatch",
                ));
            }
            order = read_external_order(shopify, &order_id, money, recording).await?;
            validate_references(&order.tags, references)?;
            if references.iter().any(|tag| !order.tags.contains(tag)) {
                return Err(reconciliation_required());
            }
        }
    }
    Ok(order)
}
async fn convert_or_reconcile_external_draft(
    shopify: &ShopifyClient,
    draft_id: &ShopifyId,
    draft: &CompletionDraft,
    money: &ExactMoney,
    mode: CompletionMode,
    recording: &ExternalPaymentRecording<'_>,
    progress: &mut impl CompletionProgress,
) -> Result<ShopifyId, AppError> {
    if let Some(order) = linked_order(draft)? {
        Ok(order)
    } else {
        if mode == CompletionMode::ReconcileOnly
            || recording.state != crate::models::checkout::PaymentRecordingState::NotStarted
        {
            return Err(reconciliation_required());
        }
        if !draft.ready || draft.payment_terms.is_some() {
            return Err(evidence_conflict(
                "Shopify draft is not eligible for pending conversion",
            ));
        }
        progress
            .before_mutation(CompletionStage::ConvertDraft)
            .await?;
        // Conversion is non-idempotent. Any uncertain response permits only linked-order reads.
        match shopify.convert_checkout_draft(draft_id, true).await {
            Ok(order) => Ok(order),
            Err(error) => {
                if std::error::Error::source(&error).is_some_and(|source| {
                    source.is::<crate::client::shopify::CheckoutIdentityMismatch>()
                }) {
                    return Err(upstream(error));
                }
                let recovered = read_draft(shopify, draft_id)
                    .await
                    .map_err(|read| reconciliation_required().with_source(read))?;
                validate_money(money, &recovered.total_price_set.presentment_money)?;
                validate_external_binding(
                    recording,
                    &recovered.tags,
                    &recovered.custom_attributes,
                )?;
                linked_order(&recovered)?
                    .ok_or_else(|| reconciliation_required().with_source(error))
            }
        }
    }
}

async fn read_external_order(
    shopify: &ShopifyClient,
    id: &ShopifyId,
    money: &ExactMoney,
    recording: &ExternalPaymentRecording<'_>,
) -> Result<CompletionOrder, AppError> {
    let order = shopify
        .checkout_order(id)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(reconciliation_required)?;
    if order.cancelled_at.is_some() {
        return Err(evidence_conflict("Shopify order was cancelled"));
    }
    validate_money(money, &order.total_price_set.presentment_money)?;
    validate_external_binding(recording, &order.tags, &order.custom_attributes)?;
    if recording.identity.is_some() && external_transactions(&order)?.len() >= 100 {
        return Err(evidence_conflict(
            "Shopify payment transactions are incomplete",
        ));
    }
    Ok(order)
}
fn validate_external_binding(
    recording: &ExternalPaymentRecording<'_>,
    tags: &[String],
    attributes: &[crate::models::shopify::CustomAttribute],
) -> Result<(), AppError> {
    if let Some(expected) = &recording.identity {
        let actual = correlation::parse_identity(attributes)?;
        correlation::validate_legacy_identity(expected, tags)?;
        let tag: String = recording.convention_slug.chars().take(40).collect();
        if actual.as_ref() != Some(expected)
            || tags.iter().filter(|actual| **actual == tag).count() != 1
        {
            return Err(evidence_conflict(
                "Shopify order does not match the accepted convention",
            ));
        }
        return Ok(());
    }
    let slug: Vec<_> = attributes
        .iter()
        .filter(|attribute| attribute.key == "convention_slug")
        .collect();
    if recording.required_tag_hashes.is_empty()
        || !recording.required_tag_hashes.iter().all(|hash| {
            tags.iter()
                .any(|tag| Sha256::digest(tag.as_bytes()).as_slice() == hash.as_slice())
        })
        || slug.len() != 1
        || slug[0].value != recording.convention_slug
    {
        return Err(evidence_conflict(
            "Shopify order does not match the accepted convention",
        ));
    }
    Ok(())
}
fn validate_external_references(
    recording: &ExternalPaymentRecording<'_>,
    tags: &[String],
    attributes: &[CustomAttribute],
    references: &[String],
    expected: &[CustomAttribute],
) -> Result<(), AppError> {
    if recording.identity.is_some() {
        validate_payment_attributes(attributes, expected, false)
    } else {
        validate_references(tags, references)
    }
}
fn payment_attribute(key: &str, value: &str) -> CustomAttribute {
    CustomAttribute {
        key: key.into(),
        value: value.into(),
    }
}
fn payment_attribute_keys() -> [(&'static str, &'static str); 5] {
    [
        (correlation::STRIPE_SESSION_KEY, "stripe:session"),
        (correlation::STRIPE_INTENT_KEY, "stripe:intent"),
        (correlation::STRIPE_CHARGE_KEY, "stripe:charge"),
        (correlation::PAYPAL_ORDER_KEY, "paypal:order"),
        (correlation::PAYPAL_CAPTURE_KEY, "paypal:capture"),
    ]
}
fn validate_payment_attributes(
    attributes: &[CustomAttribute],
    expected: &[CustomAttribute],
    require_all: bool,
) -> Result<(), AppError> {
    for (key, kind) in payment_attribute_keys() {
        let actual = correlation::unique_value(attributes, key)?;
        let wanted = expected.iter().find(|attribute| attribute.key == key);
        match (actual, wanted) {
            (Some(value), Some(wanted)) if value == wanted.value => {
                reference(kind, value)
                    .map_err(|_| evidence_conflict("Shopify payment reference is invalid"))?;
            }
            (None, Some(_)) if !require_all => {}
            (None, Some(_)) => return Err(reconciliation_required()),
            (None, None) => {}
            _ => {
                return Err(evidence_conflict(
                    "Shopify order has conflicting payment references",
                ));
            }
        }
    }
    Ok(())
}
fn validate_final_payment_attributes(
    attributes: &[CustomAttribute],
    identity: &correlation::Identity,
    resource: Option<&str>,
) -> Result<(), AppError> {
    let keys: &[&str] = match identity.provider.as_str() {
        "stripe" => &[
            correlation::STRIPE_SESSION_KEY,
            correlation::STRIPE_INTENT_KEY,
            correlation::STRIPE_CHARGE_KEY,
        ],
        "paypal" => &[
            correlation::PAYPAL_ORDER_KEY,
            correlation::PAYPAL_CAPTURE_KEY,
        ],
        _ => return Err(evidence_conflict("Checkout payment provider is invalid")),
    };
    let expected: Vec<_> = keys
        .iter()
        .map(|key| {
            correlation::unique_value(attributes, key)?
                .map(|value| payment_attribute(key, value))
                .ok_or_else(|| evidence_conflict("Shopify payment references are incomplete"))
        })
        .collect::<Result<_, _>>()?;
    if resource.is_none() || expected[0].value != resource.unwrap_or_default() {
        return Err(evidence_conflict(
            "Shopify payment resource does not match checkout",
        ));
    }
    validate_payment_attributes(attributes, &expected, true)
}
fn external_transactions(
    order: &CompletionOrder,
) -> Result<&[crate::models::shopify::CompletionTransaction], AppError> {
    order
        .transactions
        .as_deref()
        .filter(|transactions| transactions.len() <= 100)
        .ok_or_else(|| {
            evidence_conflict("Shopify payment transactions are unavailable or incomplete")
        })
}
fn validate_recordable_order(order: &CompletionOrder, money: &ExactMoney) -> Result<(), AppError> {
    if !matches!(
        order.display_financial_status,
        CompletionFinancialStatus::Pending
    ) {
        return Err(evidence_conflict(
            "Shopify order is not pending the full external payment",
        ));
    }
    let outstanding = order
        .total_outstanding_set
        .as_ref()
        .ok_or_else(|| evidence_conflict("Shopify outstanding balance is unavailable"))?;
    validate_money(money, &outstanding.presentment_money)?;
    if external_transactions(order)?.iter().any(|transaction| {
        transaction.status.eq_ignore_ascii_case("SUCCESS")
            || !transaction.kind.eq_ignore_ascii_case("SALE")
    }) {
        return Err(evidence_conflict(
            "Shopify order has conflicting payment transactions",
        ));
    }
    Ok(())
}
fn validate_recorded_order(
    order: &CompletionOrder,
    money: &ExactMoney,
    method: &str,
) -> Result<(), AppError> {
    if !matches!(
        order.display_financial_status,
        CompletionFinancialStatus::Paid
    ) {
        return Err(reconciliation_required());
    }
    let outstanding = order
        .total_outstanding_set
        .as_ref()
        .ok_or_else(|| evidence_conflict("Shopify outstanding balance is unavailable"))?;
    let zero = ExactMoney::new(0, &money.currency, money.precision)?;
    validate_money(&zero, &outstanding.presentment_money)?;
    let successful: Vec<_> = external_transactions(order)?
        .iter()
        .filter(|transaction| transaction.status.eq_ignore_ascii_case("SUCCESS"))
        .collect();
    if successful.len() != 1
        || !successful[0].gateway.eq_ignore_ascii_case(method)
        || !successful[0].kind.eq_ignore_ascii_case("SALE")
    {
        return Err(evidence_conflict(
            "Shopify payment method or transaction does not match verified external payment",
        ));
    }
    successful[0]
        .id
        .require_kind("OrderTransaction")
        .map_err(|_| evidence_conflict("Shopify payment transaction identity mismatch"))?;
    validate_money(money, &successful[0].amount_set.presentment_money)
}

enum VerifiedPayment {
    Waiting(CompletionOutcome),
    Paid {
        money: ExactMoney,
        references: Vec<String>,
        attributes: Vec<CustomAttribute>,
        method: &'static str,
    },
}
async fn verified_payment(payment: PaymentContext<'_>) -> Result<VerifiedPayment, AppError> {
    let (money, references, attributes, method) = match payment {
        PaymentContext::Stripe { client, expected } => {
            match payments::stripe::verify_payment(client, expected)
                .await
                .map_err(|error| upstream(error))?
            {
                stripe::payment::PaymentOutcome::RequiresAction
                | stripe::payment::PaymentOutcome::Pending => {
                    return Ok(VerifiedPayment::Waiting(CompletionOutcome::Pending));
                }
                stripe::payment::PaymentOutcome::ExpiredUnpaid => {
                    return Ok(VerifiedPayment::Waiting(CompletionOutcome::Unpaid));
                }
                stripe::payment::PaymentOutcome::VerifiedPaid(paid) => {
                    if paid.refunded || paid.amount_refunded != 0 || paid.disputed {
                        return Err(evidence_conflict(
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
                        vec![
                            payment_attribute(correlation::STRIPE_SESSION_KEY, &paid.session_id),
                            payment_attribute(
                                correlation::STRIPE_INTENT_KEY,
                                &paid.payment_intent_id,
                            ),
                            payment_attribute(correlation::STRIPE_CHARGE_KEY, &paid.charge_id),
                        ],
                        "Stripe Card Payments",
                    )
                }
            }
        }
        PaymentContext::Paypal { client, expected } => {
            match payments::paypal::verify_payment(client, expected)
                .await
                .map_err(|error| upstream(error))?
            {
                paypal::payment::PaymentOutcome::Pending => {
                    return Ok(VerifiedPayment::Waiting(CompletionOutcome::Pending));
                }
                paypal::payment::PaymentOutcome::TerminalUnpaid => {
                    return Ok(VerifiedPayment::Waiting(CompletionOutcome::Unpaid));
                }
                paypal::payment::PaymentOutcome::VerifiedPaid(paid) => (
                    paypal_money(paid.amount, &paid.currency)?,
                    vec![
                        reference("paypal:order", &paid.order_id)?,
                        reference("paypal:capture", &paid.capture_id)?,
                    ],
                    vec![
                        payment_attribute(correlation::PAYPAL_ORDER_KEY, &paid.order_id),
                        payment_attribute(correlation::PAYPAL_CAPTURE_KEY, &paid.capture_id),
                    ],
                    "paypal",
                ),
            }
        }
    };
    Ok(VerifiedPayment::Paid {
        money,
        references,
        attributes,
        method,
    })
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
                    ClientErrorKind::Mutation { .. } | ClientErrorKind::InvalidInput(_)
                ) || std::error::Error::source(&error).is_some_and(|source| {
                    source.is::<crate::client::shopify::CheckoutIdentityMismatch>()
                }) {
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
            return Err(evidence_conflict(
                "Shopify tag mutation order identity mismatch",
            ));
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
    let provider_scale = paypal::payment::currency_scale(currency)
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
fn validate_references(existing: &[String], expected: &[String]) -> Result<(), AppError> {
    if existing
        .iter()
        .any(|tag| tag.starts_with("payment:") && !expected.contains(tag))
    {
        return Err(evidence_conflict(
            "Shopify order has conflicting payment references",
        ));
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
                .map_err(|_| upstream(crate::client::shopify::checkout_identity_mismatch()))?;
            Ok(Some(order.id.clone()))
        }
        (CompletionDraftStatus::Open | CompletionDraftStatus::InvoiceSent, None) => Ok(None),
        _ => Err(evidence_conflict(
            "Shopify draft and linked order state mismatch",
        )),
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
        return Err(evidence_conflict(
            "Shopify order is not paid or is cancelled",
        ));
    }
    validate_money(money, &order.total_price_set.presentment_money)?;
    Ok(order)
}
fn validate_money(expected: &ExactMoney, actual: &Money) -> Result<(), AppError> {
    if actual.currency_code != expected.currency
        || expected.parse_amount(&actual.amount)? != expected.minor
    {
        return Err(evidence_conflict(
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
