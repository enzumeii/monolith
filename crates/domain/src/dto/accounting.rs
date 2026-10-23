use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingReportQuery {
    pub date_from: String,
    pub date_to: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountingReportResponse {
    pub transactions: AccountingTransactions,
    pub items: Vec<AccountingItem>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AccountingTransactions {
    pub stripe: Vec<AccountingTransaction>,
    pub paypal: Vec<AccountingTransaction>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccountingGateway {
    Paypal,
    Stripe,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccountingKind {
    Sale,
    Refund,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountingTransaction {
    pub gateway: AccountingGateway,
    #[serde(rename = "type")]
    pub kind: AccountingKind,
    pub po_number: String,
    pub po_date: Option<String>,
    pub transaction_date: String,
    pub payment_reference: String,
    pub description: String,
    pub shopify_order_id: u64,
    pub shopify_order_number: i64,
    pub customer_name: String,
    pub customer_address: String,
    pub customer_phone: u64,
    pub currency: String,
    pub exchange_rate: Option<f64>,
    pub net_paid: f64,
    pub fee: f64,
    pub vat: f64,
    pub gross_paid: f64,
    pub shipping_fee: Option<f64>,
    pub order_adjusted: bool,
    #[serde(default)]
    pub shopify_order_tags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountingItem {
    pub quantity: i64,
    pub price: f64,
    pub currency: String,
    pub title: String,
    pub variant: String,
    pub category: String,
    pub sku: String,
    pub kind: AccountingKind,
    pub transaction_date: String,
    pub shopify_order_id: u64,
    pub payment_reference: String,
}

#[cfg(test)]
#[path = "../tests/dto/accounting.rs"]
mod tests;
