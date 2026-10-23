use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct StripePage<T> {
    pub data: Vec<T>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct Payout {
    pub id: String,
    pub arrival_date: i64,
}

#[derive(Debug, Deserialize)]
pub struct FeeDetail {
    pub amount: i64,
    pub currency: String,
    pub description: Option<String>,
    pub r#type: FeeKind,
    pub application: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BalanceTransaction {
    pub amount: i64,
    pub available_on: i64,
    pub created: i64,
    pub currency: String,
    pub fee: i64,
    pub fee_details: Vec<FeeDetail>,
    pub id: String,
    pub net: i64,
    pub object: String,
    pub reporting_category: String,
    pub source: Option<String>,
    pub status: String,
    pub r#type: BalanceTransactionKind,
    pub exchange_rate: Option<f64>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub email: Option<String>,
    pub customer_email: Option<String>,
    pub order_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Charge {
    pub id: String,
    pub object: Option<String>,
    pub amount: Option<i64>,
    pub livemode: Option<bool>,
    pub status: Option<ChargeStatus>,
    pub amount_captured: i64,
    pub amount_refunded: i64,
    #[serde(flatten)]
    pub payment_status: ChargePaymentStatus,
    pub created: i64,
    pub currency: String,
    pub disputed: bool,
    pub metadata: Metadata,
    pub payment_intent: Option<String>,
    pub balance_transaction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Refund {
    pub id: String,
    pub amount: i64,
    pub created: i64,
    pub currency: String,
    pub payment_intent: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChargePaymentStatus {
    pub captured: bool,
    pub paid: bool,
    pub refunded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum BalanceTransactionKind {
    Charge,
    Payment,
    Refund,
    PaymentRefund,
    Other(String),
}

impl BalanceTransactionKind {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Charge => "charge",
            Self::Payment => "payment",
            Self::Refund => "refund",
            Self::PaymentRefund => "payment_refund",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for BalanceTransactionKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "charge" => Self::Charge,
            "payment" => Self::Payment,
            "refund" => Self::Refund,
            "payment_refund" => Self::PaymentRefund,
            _ => Self::Other(value),
        }
    }
}

impl From<BalanceTransactionKind> for String {
    fn from(value: BalanceTransactionKind) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum FeeKind {
    StripeFee,
    Tax,
    Other(String),
}

impl FeeKind {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::StripeFee => "stripe_fee",
            Self::Tax => "tax",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for FeeKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "stripe_fee" => Self::StripeFee,
            "tax" => Self::Tax,
            _ => Self::Other(value),
        }
    }
}

impl From<FeeKind> for String {
    fn from(value: FeeKind) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Debug)]
pub struct CreateCheckoutSession {
    pub amount: i64,
    pub currency: String,
    pub product_name: String,
    pub success_url: String,
    pub cancel_url: String,
    pub idempotency_key: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckoutSession {
    pub id: String,
    pub object: String,
    pub mode: CheckoutMode,
    pub status: CheckoutStatus,
    pub payment_status: CheckoutPaymentStatus,
    pub livemode: bool,
    pub amount_total: Option<i64>,
    pub currency: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub payment_intent: Option<StripeReference<PaymentIntent>>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentIntent {
    pub id: String,
    pub object: String,
    pub status: PaymentIntentStatus,
    pub amount: i64,
    pub amount_received: i64,
    pub currency: String,
    pub livemode: bool,
    pub metadata: BTreeMap<String, String>,
    pub latest_charge: Option<StripeReference<Charge>>,
}

#[derive(Debug, Deserialize)]
pub struct Account {
    pub id: String,
    pub object: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum StripeReference<T> {
    Id(String),
    Expanded(Box<T>),
}

pub trait StripeObjectId {
    fn id(&self) -> &str;
}

impl<T: StripeObjectId> StripeReference<T> {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Id(id) => id,
            Self::Expanded(object) => object.id(),
        }
    }
}

impl StripeObjectId for PaymentIntent {
    fn id(&self) -> &str {
        &self.id
    }
}

impl StripeObjectId for Charge {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum CheckoutMode {
    Payment,
    Subscription,
    Setup,
    Other(String),
}

impl CheckoutMode {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Payment => "payment",
            Self::Subscription => "subscription",
            Self::Setup => "setup",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for CheckoutMode {
    fn from(value: String) -> Self {
        match value.as_str() {
            "payment" => Self::Payment,
            "subscription" => Self::Subscription,
            "setup" => Self::Setup,
            _ => Self::Other(value),
        }
    }
}

impl From<CheckoutMode> for String {
    fn from(value: CheckoutMode) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum CheckoutStatus {
    Open,
    Complete,
    Expired,
    Other(String),
}

impl CheckoutStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::Complete => "complete",
            Self::Expired => "expired",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for CheckoutStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "open" => Self::Open,
            "complete" => Self::Complete,
            "expired" => Self::Expired,
            _ => Self::Other(value),
        }
    }
}

impl From<CheckoutStatus> for String {
    fn from(value: CheckoutStatus) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum CheckoutPaymentStatus {
    Paid,
    Unpaid,
    NoPaymentRequired,
    Other(String),
}

impl CheckoutPaymentStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Paid => "paid",
            Self::Unpaid => "unpaid",
            Self::NoPaymentRequired => "no_payment_required",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for CheckoutPaymentStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "paid" => Self::Paid,
            "unpaid" => Self::Unpaid,
            "no_payment_required" => Self::NoPaymentRequired,
            _ => Self::Other(value),
        }
    }
}

impl From<CheckoutPaymentStatus> for String {
    fn from(value: CheckoutPaymentStatus) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum PaymentIntentStatus {
    Succeeded,
    Processing,
    RequiresCapture,
    RequiresAction,
    RequiresConfirmation,
    RequiresPaymentMethod,
    Canceled,
    Other(String),
}

impl PaymentIntentStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Processing => "processing",
            Self::RequiresCapture => "requires_capture",
            Self::RequiresAction => "requires_action",
            Self::RequiresConfirmation => "requires_confirmation",
            Self::RequiresPaymentMethod => "requires_payment_method",
            Self::Canceled => "canceled",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for PaymentIntentStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "succeeded" => Self::Succeeded,
            "processing" => Self::Processing,
            "requires_capture" => Self::RequiresCapture,
            "requires_action" => Self::RequiresAction,
            "requires_confirmation" => Self::RequiresConfirmation,
            "requires_payment_method" => Self::RequiresPaymentMethod,
            "canceled" => Self::Canceled,
            _ => Self::Other(value),
        }
    }
}

impl From<PaymentIntentStatus> for String {
    fn from(value: PaymentIntentStatus) -> Self {
        value.as_str().to_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum ChargeStatus {
    Succeeded,
    Pending,
    Failed,
    Other(String),
}

impl ChargeStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Pending => "pending",
            Self::Failed => "failed",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for ChargeStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "succeeded" => Self::Succeeded,
            "pending" => Self::Pending,
            "failed" => Self::Failed,
            _ => Self::Other(value),
        }
    }
}

impl From<ChargeStatus> for String {
    fn from(value: ChargeStatus) -> Self {
        value.as_str().to_owned()
    }
}

#[cfg(test)]
#[path = "../tests/models/stripe.rs"]
mod tests;
