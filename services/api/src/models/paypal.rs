use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TransactionPage {
    pub transaction_details: Vec<TransactionDetail>,
    pub page: u32,
    pub total_items: u64,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize)]
pub struct TransactionDetail {
    #[serde(rename = "transaction_info")]
    pub transaction: TransactionInfo,
    #[serde(rename = "payer_info")]
    pub payer: Option<PayerInfo>,
    #[serde(rename = "shipping_info")]
    pub shipping: Option<ShippingInfo>,
    #[serde(rename = "cart_info")]
    pub cart: Option<CartInfo>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionInfo {
    pub transaction_id: String,
    pub transaction_event_code: String,
    pub transaction_amount: Money,
    pub transaction_initiation_date: DateTime<FixedOffset>,
    pub transaction_updated_date: DateTime<FixedOffset>,
    pub transaction_status: String,
    pub invoice_id: Option<String>,
    pub fee_amount: Option<Money>,
    pub shipping_amount: Option<Money>,
}

#[derive(Debug, Deserialize)]
pub struct Money {
    pub currency_code: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct PayerInfo {
    pub account_id: Option<String>,
    pub email_address: Option<String>,
    pub payer_name: Option<PayerName>,
    pub country_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PayerName {
    pub given_name: Option<String>,
    pub surname: Option<String>,
    pub alternate_full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShippingInfo {
    pub address: Option<Address>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Address {
    pub line1: Option<String>,
    pub city: Option<String>,
    pub country_code: Option<String>,
    pub postal_code: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CartInfo {
    pub item_details: Option<Vec<ItemDetail>>,
}

#[derive(Debug, Deserialize)]
pub struct ItemDetail {
    pub item_quantity: Option<String>,
    pub invoice_number: Option<String>,
    pub item_name: Option<String>,
    pub item_unit_price: Option<Money>,
    pub item_amount: Option<Money>,
    pub total_item_amount: Option<Money>,
}

#[cfg(test)]
#[path = "../tests/models/paypal.rs"]
mod tests;
