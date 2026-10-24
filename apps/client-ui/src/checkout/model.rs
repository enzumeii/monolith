use crate::cart::Cart;
use domain::dto::{
    ConventionQuote,
    checkout::{
        CheckoutItemInput, CheckoutNextAction, CheckoutProvider, CheckoutResponse, CheckoutStatus,
        CreateCheckoutRequest,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Attempt {
    pub version: u8,
    pub operation_id: String,
    pub secret: String,
    pub slug: String,
    pub provider: CheckoutProvider,
    pub snapshot: Cart,
    pub state: String,
    pub currency: String,
    pub total: String,
}

pub(crate) use crate::cart::valid_checkout_id as valid_id;

pub fn valid_email(email: &str) -> bool {
    let Some((local, host)) = email.split_once('@') else {
        return false;
    };
    email.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains('@')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub fn request(
    snapshot: &Cart,
    quote: &ConventionQuote,
    email: &str,
    provider: CheckoutProvider,
) -> Result<CreateCheckoutRequest, String> {
    if !valid_email(email)
        || snapshot.items.is_empty()
        || snapshot.items.len() > 100
        || quote.items.len() != snapshot.items.len()
    {
        return Err("Enter a valid confirmation email and review your current cart total.".into());
    }
    let mut seen = HashSet::new();
    let mut total = 0_i64;
    let mut items = Vec::new();
    for item in &snapshot.items {
        let line = quote
            .items
            .iter()
            .find(|line| line.variant_id == item.variant_id && line.quantity == item.quantity)
            .filter(|line| seen.insert(line.variant_id.clone()))
            .ok_or("Your cart has changed. Review its current total before paying.")?;
        let amount = minor(&line.unit_price, &quote.currency_code)
            .filter(|v| *v > 0)
            .ok_or("The current price cannot be used for payment.")?;
        if minor(&item.accepted_unit_price, &quote.currency_code) != Some(amount)
            || !(1..=999).contains(&item.quantity)
        {
            return Err("Prices have changed. Accept the updated prices before paying.".into());
        }
        let line_total = amount
            .checked_mul(i64::from(item.quantity))
            .ok_or("The cart total is too large.")?;
        if minor(&line.line_total, &quote.currency_code) != Some(line_total) {
            return Err("The shop returned an invalid total. Please refresh your cart.".into());
        }
        total = total
            .checked_add(line_total)
            .ok_or("The cart total is too large.")?;
        items.push(CheckoutItemInput {
            variant_id: item.variant_id.clone(),
            quantity: item.quantity,
            expected_unit_price: amount,
        });
    }
    if minor(&quote.total, &quote.currency_code) != Some(total)
        || minor(&quote.subtotal, &quote.currency_code) != Some(total)
        || minor(&quote.shipping, &quote.currency_code) != Some(0)
    {
        return Err("The shop returned an invalid total. Please refresh your cart.".into());
    }
    Ok(CreateCheckoutRequest {
        operation_id: String::new(),
        convention_id: snapshot.convention_id.clone(),
        email: email.into(),
        provider,
        items,
        expected_currency: quote.currency_code.clone(),
    })
}

pub fn saved_request_matches(attempt: &Attempt, request: &CreateCheckoutRequest) -> bool {
    if request.operation_id != attempt.operation_id
        || request.provider != attempt.provider
        || request.convention_id != attempt.snapshot.convention_id
        || request.expected_currency != attempt.currency
        || !valid_email(&request.email)
        || request.items.len() != attempt.snapshot.items.len()
        || request.items.is_empty()
    {
        return false;
    }
    let mut seen = HashSet::new();
    let total = request.items.iter().try_fold(0_i64, |total, item| {
        let saved =
            attempt.snapshot.items.iter().find(|saved| {
                saved.variant_id == item.variant_id && saved.quantity == item.quantity
            })?;
        if !seen.insert(&item.variant_id)
            || minor(&saved.accepted_unit_price, &attempt.currency)
                != Some(item.expected_unit_price)
        {
            return None;
        }
        total.checked_add(
            item.expected_unit_price
                .checked_mul(i64::from(item.quantity))?,
        )
    });
    total.is_some() && total == minor(&attempt.total, &attempt.currency)
}

pub fn checked_response(attempt: &Attempt, response: &CheckoutResponse) -> Result<(), String> {
    if response.checkout_id != attempt.operation_id
        || response.money.currency != attempt.currency
        || minor(&attempt.total, &attempt.currency) != Some(response.money.minor)
    {
        return Err(
            "The shop returned a different checkout. Keep your receipt and contact the shop."
                .into(),
        );
    }
    if response.status == CheckoutStatus::Completed
        && response
            .order
            .as_ref()
            .is_none_or(|order| order.id.is_empty() || order.name.trim().is_empty())
    {
        return Err(
            "Your payment is being checked. The order confirmation is not available yet.".into(),
        );
    }
    Ok(())
}

pub fn redirect(
    response: &CheckoutResponse,
    provider: CheckoutProvider,
    test_origin: Option<&str>,
) -> Result<Option<String>, String> {
    if response.status != CheckoutStatus::RequiresAction {
        return Ok(None);
    }
    let Some(CheckoutNextAction::Redirect { url }) = &response.next_action else {
        return Err("The payment link is unavailable. Check this checkout again.".into());
    };
    let parsed =
        reqwest::Url::parse(url).map_err(|_| "The shop returned an invalid payment link.")?;
    let safe = parsed.scheme() == "https"
        && parsed.port_or_known_default() == Some(443)
        && match provider {
            CheckoutProvider::Stripe => parsed.host_str() == Some("checkout.stripe.com"),
            CheckoutProvider::Paypal => matches!(
                parsed.host_str(),
                Some("www.paypal.com" | "www.sandbox.paypal.com")
            ),
        };
    let controlled_local = test_origin
        .and_then(|origin| reqwest::Url::parse(origin).ok())
        .is_some_and(|origin| {
            matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
                && origin.origin() == parsed.origin()
                && matches!(parsed.scheme(), "http" | "https")
        });
    if (!safe && !controlled_local)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || (parsed.fragment().is_some() && !(safe && provider == CheckoutProvider::Stripe))
    {
        return Err(
            "The shop returned an unsupported payment link. Check this checkout again.".into(),
        );
    }
    Ok(Some(url.clone()))
}

fn minor(value: &str, currency: &str) -> Option<i64> {
    if currency.len() != 3 || !currency.bytes().all(|v| v.is_ascii_uppercase()) {
        return None;
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|v| v.is_ascii_digit())
        || !fraction.bytes().all(|v| v.is_ascii_digit())
        || value.ends_with('.')
        || fraction.len() > 3
    {
        return None;
    }
    format!("{whole}{fraction}").parse().ok()
}

#[cfg(test)]
#[path = "../tests/checkout/model.rs"]
mod tests;
