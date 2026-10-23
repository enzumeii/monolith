use crate::{error::AppError, models::shopify::ShopifyId};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExactMoney {
    pub minor: i64,
    pub currency: String,
    pub precision: u8,
}
impl ExactMoney {
    #[allow(
        dead_code,
        reason = "Upstream checkout creation is integrated on the later dependency rebase"
    )]
    pub(crate) fn new(minor: i64, currency: &str, precision: u8) -> Result<Self, AppError> {
        let money = Self {
            minor,
            currency: currency.into(),
            precision,
        };
        money.validate()?;
        Ok(money)
    }
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.minor < 0
            || currency_precision(&self.currency) != Some(self.precision)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
        {
            return Err(conflict("Invalid exact payment amount"));
        }
        Ok(())
    }
    pub(crate) fn parse_amount(&self, value: &str) -> Result<i64, AppError> {
        self.validate()?;
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || value.ends_with('.')
        {
            return Err(conflict("Invalid exact payment amount"));
        }
        let places = usize::from(self.precision);
        if fraction
            .get(places..)
            .is_some_and(|tail| tail.bytes().any(|b| b != b'0'))
        {
            return Err(conflict("Payment precision mismatch"));
        }
        let scale = 10_i64.pow(u32::from(self.precision));
        let whole = whole
            .parse::<i64>()
            .ok()
            .and_then(|v| v.checked_mul(scale))
            .ok_or_else(|| conflict("Payment amount overflow"))?;
        let digits = &fraction[..fraction.len().min(places)];
        let fraction = if digits.is_empty() {
            0
        } else {
            digits
                .parse::<i64>()
                .map_err(|_| conflict("Invalid exact payment amount"))?
        };
        let padding =
            u32::try_from(places - digits.len()).map_err(|_| conflict("Invalid precision"))?;
        whole
            .checked_add(fraction * 10_i64.pow(padding))
            .ok_or_else(|| conflict("Payment amount overflow"))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotLine {
    pub variant_id: ShopifyId,
    pub quantity: i64,
    pub unit_price: ExactMoney,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub draft_id: ShopifyId,
    pub accepted: ExactMoney,
    pub lines: Vec<SnapshotLine>,
    pub email: String,
    pub phone: Option<String>,
    pub attributes: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub reservation_until: DateTime<Utc>,
}
impl Snapshot {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        self.draft_id
            .require_kind("DraftOrder")
            .map_err(|_| conflict("Invalid reserved draft"))?;
        self.accepted.validate()?;
        if self.accepted.minor == 0
            || self.email.trim().is_empty()
            || self.lines.is_empty()
            || self.lines.len() > 250
            || self.tags.len() < 2
            || self.tags.iter().any(|tag| {
                tag.trim().is_empty()
                    || tag.eq_ignore_ascii_case("PACKING")
                    || tag.eq_ignore_ascii_case("SHIPPING")
            })
            || self.attributes.is_empty()
        {
            return Err(conflict("Incomplete reserved checkout snapshot"));
        }
        let mut total = 0_i64;
        let mut variants = std::collections::HashSet::new();
        for line in &self.lines {
            line.variant_id
                .require_kind("ProductVariant")
                .map_err(|_| conflict("Invalid reserved variant"))?;
            line.unit_price.validate()?;
            if line.quantity <= 0
                || line.quantity > i64::from(i32::MAX)
                || !variants.insert(line.variant_id.as_ref())
                || line.unit_price.currency != self.accepted.currency
                || line.unit_price.precision != self.accepted.precision
            {
                return Err(conflict("Invalid reserved checkout line"));
            }
            total = line
                .unit_price
                .minor
                .checked_mul(line.quantity)
                .and_then(|v| total.checked_add(v))
                .ok_or_else(|| conflict("Checkout total overflow"))?;
        }
        if total != self.accepted.minor {
            return Err(conflict("Checkout total mismatch"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Provider {
    Stripe,
    Paypal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PaymentEnvironment {
    Test,
    Live,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaymentAttempt {
    pub attempt_id: String,
    pub provider: Provider,
    pub account: String,
    pub environment: PaymentEnvironment,
    pub reference: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AliasKind {
    StripeSession,
    StripePaymentIntent,
    StripeCharge,
    PaypalOrder,
    PaypalCapture,
}
impl AliasKind {
    pub(crate) fn provider(self) -> Provider {
        match self {
            Self::StripeSession | Self::StripePaymentIntent | Self::StripeCharge => {
                Provider::Stripe
            }
            Self::PaypalOrder | Self::PaypalCapture => Provider::Paypal,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaymentAlias {
    pub kind: AliasKind,
    pub value: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PaymentFact {
    Pending,
    Unpaid,
    Expired,
    Paid {
        money: ExactMoney,
        aliases: Vec<PaymentAlias>,
    },
}
// This integration input deliberately has no Deserialize implementation: only trusted server adapters construct it.
#[derive(Clone, Serialize)]
pub(crate) struct VerifiedPayment {
    pub checkout_id: Uuid,
    pub draft_id: ShopifyId,
    pub attempt: PaymentAttempt,
    pub event_id: String,
    pub state: PaymentFact,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PaymentState {
    Pending,
    Unpaid,
    Expired,
    Paid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum CompletionState {
    Ready,
    Processing,
    Uncertain,
    OrderSaved,
    Complete,
    Operator,
}
#[derive(Clone)]
pub(crate) struct CompletionRow {
    pub id: Uuid,
    pub snapshot: Snapshot,
    pub attempt: PaymentAttempt,
    pub guest_token_hash: Vec<u8>,
    pub guest_token_expires_at: DateTime<Utc>,
    pub guest_token_revoked_at: Option<DateTime<Utc>>,
    pub next_verify_at: Option<DateTime<Utc>>,
    pub payment_state: PaymentState,
    pub completion_state: CompletionState,
    pub trusted_payment: Option<PaymentFact>,
    pub order_id: Option<ShopifyId>,
    pub completion_intent: bool,
    pub lease_token: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug)]
pub(crate) struct CompletionOutcome {
    pub payment_state: PaymentState,
    pub completion_state: CompletionState,
    pub order_id: Option<ShopifyId>,
}
impl From<&CompletionRow> for CompletionOutcome {
    fn from(row: &CompletionRow) -> Self {
        Self {
            payment_state: row.payment_state,
            completion_state: row.completion_state,
            order_id: row.order_id.clone(),
        }
    }
}
pub(crate) fn currency_precision(currency: &str) -> Option<u8> {
    use crate::models::CurrencyCode;
    match CurrencyCode::from(currency.to_owned()) {
        CurrencyCode::JPY => Some(0),
        CurrencyCode::BHD
        | CurrencyCode::IQD
        | CurrencyCode::JOD
        | CurrencyCode::KWD
        | CurrencyCode::LYD
        | CurrencyCode::OMR
        | CurrencyCode::TND => Some(3),
        CurrencyCode::Other(_) => None,
        _ => Some(2),
    }
}
pub(crate) fn conflict(message: &'static str) -> AppError {
    AppError::new(StatusCode::CONFLICT, message)
}
#[cfg(test)]
#[path = "../tests/models/checkout_completion.rs"]
mod tests;
