use serde::{Deserialize, Serialize};

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

#[cfg(test)]
#[path = "../tests/models/stripe.rs"]
mod tests;
