use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::{
        checkout_completion::{
            AliasKind, CompletionOutcome, CompletionRow, CompletionState, ExactMoney, PaymentAlias,
            PaymentAttempt, PaymentFact, PaymentState, Provider, Snapshot, VerifiedPayment,
            conflict,
        },
        shopify::{
            CompletionAttribute, CompletionDraft, CompletionDraftStatus, CompletionFinancialStatus,
            CompletionFulfillmentStatus, CompletionLine, CompletionMetafield, CompletionMoneyBag,
            CompletionOrder, Connection, Money, ShopifyId,
        },
    },
    repositories::checkout_completions as repo,
};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

#[allow(
    dead_code,
    reason = "Verified ENZ-43/44 webhook integration is deferred; the server-only seam is tested over SQL and TCP"
)]
pub(crate) async fn apply_verified_payment(
    pool: &PgPool,
    shopify: &ShopifyClient,
    payment: &VerifiedPayment,
) -> Result<CompletionOutcome, AppError> {
    let token = Uuid::new_v4();
    let mut transaction = pool.begin().await?;
    let claimed = repo::claim(
        &mut transaction,
        payment.checkout_id,
        token,
        Utc::now() + chrono::Duration::seconds(120),
    )
    .await?;
    transaction.commit().await?;
    let Some(row) = claimed else {
        repo::find(pool, payment.checkout_id)
            .await?
            .ok_or_else(not_found)?;
        return Err(AppError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Checkout completion is already processing",
        ));
    };
    apply_owned_payment(pool, shopify, payment, token, row).await
}

pub(crate) async fn apply_claimed_verified_payment(
    pool: &PgPool,
    shopify: &ShopifyClient,
    payment: &VerifiedPayment,
    lease_token: Uuid,
) -> Result<CompletionOutcome, AppError> {
    let row = repo::find(pool, payment.checkout_id)
        .await?
        .ok_or_else(not_found)?;
    if row.lease_token != Some(lease_token)
        || row.lease_until.is_none_or(|until| until <= Utc::now())
    {
        return Err(conflict("Completion ownership expired"));
    }
    apply_owned_payment(pool, shopify, payment, lease_token, row).await
}

async fn apply_owned_payment(
    pool: &PgPool,
    shopify: &ShopifyClient,
    payment: &VerifiedPayment,
    token: Uuid,
    mut row: CompletionRow,
) -> Result<CompletionOutcome, AppError> {
    let effective = match eligible_payment(&row, payment) {
        Ok(effective) => effective,
        Err(error) => {
            repo::save_operator_payment(pool, row.id, token, payment).await?;
            return Err(error);
        }
    };
    if row.completion_state == CompletionState::Operator {
        repo::progress(
            pool,
            row.id,
            token,
            CompletionState::Operator,
            None,
            false,
            Some("Checkout requires operator review"),
            true,
        )
        .await?;
        return Err(conflict("Checkout requires operator review"));
    }
    if let Some(effective) = effective {
        let cached_complete = row.completion_state == CompletionState::Complete
            && row.trusted_payment.as_ref() == Some(&effective.state);
        let mut transaction = pool.begin().await?;
        let saved = async {
            let PaymentFact::Paid { aliases, .. } = &effective.state else {
                return Err(conflict("Paid handoff required"));
            };
            repo::bind_aliases(&mut transaction, row.id, &row.attempt, aliases).await?;
            repo::record_fact(
                &mut transaction,
                row.id,
                token,
                &effective.state,
                &effective.event_id,
            )
            .await
        }
        .await;
        match saved {
            Ok(()) => transaction.commit().await?,
            Err(error) => {
                transaction.rollback().await?;
                if error.http_status() == axum::http::StatusCode::CONFLICT {
                    repo::save_operator_payment(pool, row.id, token, payment).await?;
                } else {
                    save_completion_failure(pool, row.id, token, &error).await?;
                }
                return Err(error);
            }
        }
        if cached_complete {
            repo::progress(
                pool,
                row.id,
                token,
                CompletionState::Complete,
                None,
                false,
                None,
                true,
            )
            .await?;
            return Ok(CompletionOutcome::from(&row));
        }
        row.payment_state = PaymentState::Paid;
        row.trusted_payment = Some(effective.state.clone());
        let result = complete_paid(pool, shopify, &row, token, &effective).await;
        if let Err(error) = &result {
            save_completion_failure(pool, row.id, token, error).await?;
        }
        result
    } else {
        if row.payment_state == PaymentState::Pending {
            repo::record_fact(
                &mut *pool.acquire().await?,
                row.id,
                token,
                &payment.state,
                &payment.event_id,
            )
            .await?;
        }
        repo::progress(
            pool,
            row.id,
            token,
            row.completion_state,
            None,
            false,
            None,
            true,
        )
        .await?;
        saved_outcome(pool, row.id).await
    }
}

async fn save_completion_failure(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    error: &AppError,
) -> Result<(), AppError> {
    let current = repo::find(pool, id).await?.ok_or_else(not_found)?;
    if current.lease_token == Some(token) {
        let (state, message) = if error.http_status() == axum::http::StatusCode::CONFLICT {
            (
                CompletionState::Operator,
                "Checkout requires operator review",
            )
        } else if current.order_id.is_some() {
            (CompletionState::OrderSaved, "Order verification pending")
        } else if current.completion_intent {
            (CompletionState::Uncertain, "Completion outcome uncertain")
        } else {
            (CompletionState::Processing, "Completion read pending")
        };
        repo::progress(pool, id, token, state, None, false, Some(message), true).await?;
    }
    Ok(())
}

async fn complete_paid(
    pool: &PgPool,
    shopify: &ShopifyClient,
    row: &CompletionRow,
    token: Uuid,
    payment: &VerifiedPayment,
) -> Result<CompletionOutcome, AppError> {
    let draft = shopify
        .checkout_draft(&row.snapshot.draft_id)
        .await
        .map_err(upstream)?
        .ok_or_else(|| conflict("Reserved draft is unavailable"))?;
    let known = if let Some(order) = draft.order.as_ref() {
        order.id.require_kind("Order").map_err(upstream)?;
        if row
            .order_id
            .as_ref()
            .is_some_and(|saved| saved != &order.id)
        {
            return Err(conflict("Draft order conflicts with saved order"));
        }
        order.id.clone()
    } else {
        if row.order_id.is_some() {
            return Err(conflict("Saved order is missing from draft"));
        }
        if row.completion_intent || matches!(draft.status, CompletionDraftStatus::Completed) {
            repo::progress(
                pool,
                row.id,
                token,
                CompletionState::Uncertain,
                None,
                true,
                Some("Draft completion outcome uncertain"),
                true,
            )
            .await?;
            return saved_outcome(pool, row.id).await;
        }
        validate_draft(&row.snapshot, &draft)?;
        repo::progress(
            pool,
            row.id,
            token,
            CompletionState::Processing,
            None,
            true,
            None,
            false,
        )
        .await?;
        shopify
            .complete_checkout_draft(&row.snapshot.draft_id)
            .await
            .map_err(upstream)?
    };
    repo::progress(
        pool,
        row.id,
        token,
        CompletionState::OrderSaved,
        Some(&known),
        true,
        None,
        false,
    )
    .await?;
    verify_order_metadata(shopify, &row.snapshot, &known, payment, row.id).await?;
    repo::progress(
        pool,
        row.id,
        token,
        CompletionState::Complete,
        Some(&known),
        true,
        None,
        true,
    )
    .await?;
    saved_outcome(pool, row.id).await
}
async fn verify_order_metadata(
    shopify: &ShopifyClient,
    snapshot: &Snapshot,
    known: &ShopifyId,
    payment: &VerifiedPayment,
    checkout: Uuid,
) -> Result<(), AppError> {
    let order = shopify
        .checkout_order(known)
        .await
        .map_err(upstream)?
        .ok_or_else(|| conflict("Completed order is unavailable"))?;
    validate_order(snapshot, &order)?;
    let metadata = payment_metadata(checkout, payment)?;
    let existing = metadata_value(order.metafield.as_ref())?;
    if existing.as_ref() != Some(&metadata) {
        if existing
            .as_ref()
            .is_some_and(|existing| !metadata_is_extension(existing, &metadata))
        {
            return Err(conflict("Payment metadata conflicts with saved payment"));
        }
        shopify
            .set_checkout_payment_metadata(
                known,
                &metadata,
                order
                    .metafield
                    .as_ref()
                    .map(|field| field.compare_digest.as_str()),
            )
            .await
            .map_err(upstream)?;
        let confirmed = shopify
            .checkout_order(known)
            .await
            .map_err(upstream)?
            .ok_or_else(|| conflict("Completed order is unavailable"))?;
        validate_order(snapshot, &confirmed)?;
        if metadata_value(confirmed.metafield.as_ref())? != Some(metadata) {
            return Err(upstream(crate::client::ClientError::new(
                crate::client::ClientErrorKind::Protocol(
                    "payment metadata readback is not yet confirmed",
                ),
            )));
        }
    }
    Ok(())
}

fn eligible_payment(
    row: &CompletionRow,
    payment: &VerifiedPayment,
) -> Result<Option<VerifiedPayment>, AppError> {
    row.snapshot.validate()?;
    if row.id != payment.checkout_id
        || row.snapshot.draft_id != payment.draft_id
        || row.attempt != payment.attempt
        || payment.event_id.trim().is_empty()
        || row.attempt.attempt_id.trim().is_empty()
        || row.attempt.reference.trim().is_empty()
        || row.attempt.account.trim().is_empty()
    {
        return Err(conflict("Payment association mismatch"));
    }
    let mut effective = payment.clone();
    match (&payment.state, &row.trusted_payment) {
        (PaymentFact::Paid { money, aliases }, saved) => {
            validate_aliases(&row.attempt, aliases)?;
            let mut sorted = aliases.clone();
            sorted.sort_by_key(|alias| alias.kind);
            effective.state = PaymentFact::Paid {
                money: money.clone(),
                aliases: sorted,
            };
            if money != &row.snapshot.accepted {
                return Err(conflict("Payment amount or currency mismatch"));
            }
            if let Some(PaymentFact::Paid {
                money: saved_money,
                aliases: saved_aliases,
            }) = saved
            {
                if saved_money != money
                    || saved_aliases.iter().any(|old| {
                        aliases
                            .iter()
                            .any(|new| new.kind == old.kind && new.value != old.value)
                    })
                {
                    return Err(conflict(
                        "Another successful payment requires operator review",
                    ));
                }
                let mut merged = saved_aliases.clone();
                for alias in aliases {
                    if !merged.contains(alias) {
                        merged.push(alias.clone());
                    }
                }
                merged.sort_by_key(|alias| alias.kind);
                effective.state = PaymentFact::Paid {
                    money: money.clone(),
                    aliases: merged,
                };
            }
            Ok(Some(effective))
        }
        (_, Some(saved @ PaymentFact::Paid { .. })) => {
            effective.state = saved.clone();
            Ok(Some(effective))
        }
        _ => Ok(None),
    }
}
fn validate_aliases(attempt: &PaymentAttempt, aliases: &[PaymentAlias]) -> Result<(), AppError> {
    let mut kinds = std::collections::BTreeSet::new();
    if aliases.is_empty()
        || aliases.len() > 5
        || aliases.iter().any(|alias| {
            alias.kind.provider() != attempt.provider
                || alias.value.trim().is_empty()
                || !kinds.insert(alias.kind)
        })
        || !aliases.iter().any(|alias| alias.value == attempt.reference)
    {
        return Err(conflict("Payment reference association mismatch"));
    }
    let settlement = match attempt.provider {
        Provider::Stripe => AliasKind::StripeCharge,
        Provider::Paypal => AliasKind::PaypalCapture,
    };
    if !kinds.contains(&settlement) {
        return Err(conflict("Settled payment reference is missing"));
    }
    Ok(())
}
fn validate_draft(snapshot: &Snapshot, draft: &CompletionDraft) -> Result<(), AppError> {
    if !draft.ready
        || matches!(draft.status, CompletionDraftStatus::Completed)
        || draft.payment_terms.is_some()
        || draft
            .reserve_inventory_until
            .is_none_or(|until| until <= Utc::now() || until != snapshot.reservation_until)
        || snapshot.reservation_until <= Utc::now()
    {
        return Err(conflict("Draft reservation is no longer eligible"));
    }
    validate_contents(
        snapshot,
        draft.email.as_deref(),
        draft.phone.as_deref(),
        &draft.custom_attributes,
        &draft.tags,
        draft.taxes_included,
        &draft.total_price_set,
        &draft.total_shipping_price_set,
        &draft.line_items,
    )
}
fn validate_order(snapshot: &Snapshot, order: &CompletionOrder) -> Result<(), AppError> {
    if !matches!(
        order.display_financial_status,
        CompletionFinancialStatus::Paid
    ) || !matches!(
        order.display_fulfillment_status,
        CompletionFulfillmentStatus::Unfulfilled
    ) || order.cancelled_at.is_some()
    {
        return Err(conflict("Completed order payment or fulfillment mismatch"));
    }
    validate_contents(
        snapshot,
        order.email.as_deref(),
        order.phone.as_deref(),
        &order.custom_attributes,
        &order.tags,
        order.taxes_included,
        &order.total_price_set,
        &order.total_shipping_price_set,
        &order.line_items,
    )
}
#[allow(clippy::too_many_arguments)]
fn validate_contents(
    snapshot: &Snapshot,
    email: Option<&str>,
    phone: Option<&str>,
    attributes: &[CompletionAttribute],
    tags: &[String],
    taxes_included: bool,
    total: &CompletionMoneyBag,
    shipping: &CompletionMoneyBag,
    lines: &Connection<CompletionLine>,
) -> Result<(), AppError> {
    let actual_attributes: BTreeMap<_, _> = attributes
        .iter()
        .map(|attribute| (attribute.key.as_str(), attribute.value.as_str()))
        .collect();
    let expected_attributes: BTreeMap<_, _> = snapshot
        .attributes
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut actual_tags = tags.to_vec();
    actual_tags.sort();
    let mut expected_tags = snapshot.tags.clone();
    expected_tags.sort();
    if email != Some(snapshot.email.as_str())
        || phone != snapshot.phone.as_deref()
        || actual_attributes != expected_attributes
        || actual_attributes.len() != attributes.len()
        || actual_tags != expected_tags
        || tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("PACKING") || tag.eq_ignore_ascii_case("SHIPPING"))
        || !taxes_included
        || !money_matches(
            &snapshot.accepted,
            &total.presentment_money,
            snapshot.accepted.minor,
        )?
        || !money_matches(&snapshot.accepted, &shipping.presentment_money, 0)?
        || lines.page_info.has_next_page
        || lines.nodes.len() != snapshot.lines.len()
    {
        return Err(conflict("Shopify checkout snapshot mismatch"));
    }
    let mut seen = std::collections::HashSet::new();
    for line in &lines.nodes {
        let variant = line
            .variant
            .as_ref()
            .ok_or_else(|| conflict("Checkout variant is missing"))?;
        let expected = snapshot
            .lines
            .iter()
            .find(|expected| expected.variant_id == variant.id)
            .ok_or_else(|| conflict("Checkout variant mismatch"))?;
        if !seen.insert(variant.id.as_ref())
            || line.quantity != expected.quantity
            || !money_matches(
                &expected.unit_price,
                &line.original_unit_price_set.presentment_money,
                expected.unit_price.minor,
            )?
        {
            return Err(conflict("Checkout line mismatch"));
        }
    }
    Ok(())
}
fn money_matches(expected: &ExactMoney, actual: &Money, minor: i64) -> Result<bool, AppError> {
    Ok(actual.currency_code == expected.currency
        && expected.parse_amount(&actual.amount).map_err(|_| {
            upstream(crate::client::ClientError::new(
                crate::client::ClientErrorKind::Protocol("invalid Shopify checkout amount"),
            ))
        })? == minor)
}
fn payment_metadata(checkout: Uuid, payment: &VerifiedPayment) -> Result<Value, AppError> {
    let PaymentFact::Paid { money, aliases } = &payment.state else {
        return Err(conflict("Paid handoff required"));
    };
    let mut aliases = aliases.clone();
    aliases.sort_by_key(|alias| alias.kind);
    Ok(
        json!({"checkout_id":checkout,"draft_id":payment.draft_id,"attempt_id":payment.attempt.attempt_id,"provider":payment.attempt.provider,"account":payment.attempt.account,"environment":payment.attempt.environment,"amount":money,"aliases":aliases}),
    )
}
fn metadata_is_extension(existing: &Value, target: &Value) -> bool {
    let (Some(mut existing), Some(mut target)) =
        (existing.as_object().cloned(), target.as_object().cloned())
    else {
        return false;
    };
    let (Some(Value::Array(old)), Some(Value::Array(new))) =
        (existing.remove("aliases"), target.remove("aliases"))
    else {
        return false;
    };
    existing == target && old.iter().all(|alias| new.contains(alias))
}
fn metadata_value(field: Option<&CompletionMetafield>) -> Result<Option<Value>, AppError> {
    field
        .map(|field| {
            serde_json::from_str(&field.value).map_err(|_| conflict("Payment metadata is invalid"))
        })
        .transpose()
}
async fn saved_outcome(pool: &PgPool, id: Uuid) -> Result<CompletionOutcome, AppError> {
    let row = repo::find(pool, id).await?.ok_or_else(not_found)?;
    Ok(CompletionOutcome::from(&row))
}
fn not_found() -> AppError {
    AppError::new(axum::http::StatusCode::NOT_FOUND, "Checkout not found")
}
#[cfg(test)]
#[path = "../tests/services/checkout_completion.rs"]
pub(crate) mod tests;
