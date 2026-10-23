use super::{Payment, protocol};
use crate::{
    client::{paypal::PaypalClient, stripe::StripeClient},
    error::{AppError, upstream},
    models::{
        checkout_completion::{AliasKind, PaymentEnvironment, Provider},
        paypal::EndpointEnvironment,
    },
    repositories::checkout_completions::{self, AliasMapping},
};
use domain::dto::{AccountingGateway, AccountingKind};
use sqlx::PgPool;
use std::collections::HashMap;

pub(super) async fn mappings(
    pool: &PgPool,
    stripe: &StripeClient,
    paypal: &PaypalClient,
    payments: &[Payment],
) -> Result<HashMap<(u8, String), AliasMapping>, AppError> {
    let mut result = HashMap::new();
    for (gateway, provider, key) in [
        (AccountingGateway::Stripe, Provider::Stripe, 0),
        (AccountingGateway::Paypal, Provider::Paypal, 1),
    ] {
        let references = payments
            .iter()
            .filter(|p| {
                p.transaction.gateway == gateway && p.transaction.kind == AccountingKind::Sale
            })
            .map(|p| p.transaction.payment_reference.clone())
            .collect::<Vec<_>>();
        if references.is_empty() {
            continue;
        }
        for mapping in checkout_completions::lookup_aliases(pool, provider, &references).await? {
            let index = (key, mapping.value.clone());
            if let Some(previous) = result.get(&index) {
                let previous: &AliasMapping = previous;
                if previous.checkout_id != mapping.checkout_id
                    || previous.account != mapping.account
                    || previous.environment != mapping.environment
                    || previous.kind != mapping.kind
                    || previous.order_id != mapping.order_id
                {
                    return Err(protocol("ambiguous checkout accounting reference"));
                }
            } else {
                result.insert(index, mapping);
            }
        }
    }
    let mut stripe_account = None;
    let mut paypal_orders = HashMap::new();
    for ((gateway, _), mapping) in &result {
        if mapping.order_id.is_none() {
            continue;
        }
        mapping.snapshot.validate()?;
        if mapping.attempt.account != mapping.account
            || mapping.attempt.environment != mapping.environment
            || mapping.kind.provider() != mapping.attempt.provider
        {
            return Err(protocol("checkout accounting provider identity mismatch"));
        }
        if *gateway == 0 {
            validate_stripe(stripe, mapping, &mut stripe_account).await?;
        } else {
            validate_paypal(paypal, mapping, &mut paypal_orders).await?;
        }
    }
    Ok(result)
}

async fn validate_stripe(
    client: &StripeClient,
    mapping: &AliasMapping,
    account: &mut Option<String>,
) -> Result<(), AppError> {
    if mapping.kind != AliasKind::StripeCharge {
        return Err(protocol("checkout accounting Stripe alias mismatch"));
    }
    if account.is_none() {
        *account = Some(client.get_account().await.map_err(upstream)?.id);
    }
    let charge = client.get_charge(&mapping.value).await.map_err(upstream)?;
    let environment = charge.livemode.map(|live| {
        if live {
            PaymentEnvironment::Live
        } else {
            PaymentEnvironment::Test
        }
    });
    let accepted = &mapping.snapshot.accepted;
    if account.as_deref() != Some(mapping.account.as_str())
        || environment != Some(mapping.environment)
        || charge.id != mapping.value
        || !charge.payment_status.paid
        || !charge.payment_status.captured
        || charge.amount_captured != accepted.minor
        || !charge.currency.eq_ignore_ascii_case(&accepted.currency)
    {
        return Err(protocol("checkout accounting Stripe association mismatch"));
    }
    Ok(())
}

async fn validate_paypal(
    client: &PaypalClient,
    mapping: &AliasMapping,
    orders: &mut HashMap<String, crate::models::paypal::Order>,
) -> Result<(), AppError> {
    let environment = client
        .endpoint_environment()
        .map(|environment| match environment {
            EndpointEnvironment::Sandbox => PaymentEnvironment::Test,
            EndpointEnvironment::Live => PaymentEnvironment::Live,
        });
    if mapping.kind != AliasKind::PaypalCapture || environment != Some(mapping.environment) {
        return Err(protocol("checkout accounting PayPal association mismatch"));
    }
    if let std::collections::hash_map::Entry::Vacant(entry) =
        orders.entry(mapping.attempt.reference.clone())
    {
        entry.insert(
            client
                .get_order(&mapping.attempt.reference)
                .await
                .map_err(upstream)?,
        );
    }
    let order = orders
        .get(&mapping.attempt.reference)
        .ok_or_else(|| protocol("checkout accounting PayPal order missing"))?;
    let accepted = &mapping.snapshot.accepted;
    let mut matched = 0;
    for unit in &order.purchase_units {
        for capture in unit.payments.iter().flat_map(|p| &p.captures) {
            if capture.id != mapping.value {
                continue;
            }
            matched += 1;
            if unit.payee.as_ref().map(|p| p.merchant_id.as_str()) != Some(mapping.account.as_str())
                || crate::models::CurrencyCode::from(capture.amount.currency_code.clone()).as_str()
                    != accepted.currency
                || accepted
                    .parse_amount(&capture.amount.value)
                    .map_err(|_| protocol("invalid checkout accounting PayPal amount"))?
                    != accepted.minor
            {
                return Err(protocol("checkout accounting PayPal association mismatch"));
            }
        }
    }
    if order.id != mapping.attempt.reference || matched != 1 {
        return Err(protocol("checkout accounting PayPal capture mismatch"));
    }
    Ok(())
}
