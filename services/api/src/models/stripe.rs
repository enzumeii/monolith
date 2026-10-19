use serde::Deserialize;

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
    pub r#type: String,
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
    pub r#type: String,
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
