pub mod payment;
mod payments;
pub mod webhook;
pub use payments::*;

use super::CurrencyCode;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

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
    pub transaction_event_code: TransactionEvent,
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
    pub currency_code: CurrencyCode,
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum TransactionEvent {
    BankWithdrawal,
    ManualWithdrawal,
    UserCurrencyConversion,
    CurrencyConversion,
    Refund,
    PaymentReviewHold,
    PaymentReviewRelease,
    DisputeHold,
    DisputeHoldCancellation,
    PaymentReversal,
    DisputeFee,
    GeneralPayment,
    MobilePayment,
    ExpressCheckoutPayment,
    Other(String),
}

impl TransactionEvent {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::BankWithdrawal => "T0400",
            Self::ManualWithdrawal => "T0403",
            Self::UserCurrencyConversion => "T0201",
            Self::CurrencyConversion => "T0200",
            Self::Refund => "T1107",
            Self::PaymentReviewHold => "T2105",
            Self::PaymentReviewRelease => "T2106",
            Self::DisputeHold => "T1110",
            Self::DisputeHoldCancellation => "T1111",
            Self::PaymentReversal => "T1106",
            Self::DisputeFee => "T0114",
            Self::GeneralPayment => "T0000",
            Self::MobilePayment => "T0011",
            Self::ExpressCheckoutPayment => "T0006",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for TransactionEvent {
    fn from(value: String) -> Self {
        match value.as_str() {
            "T0400" => Self::BankWithdrawal,
            "T0403" => Self::ManualWithdrawal,
            "T0201" => Self::UserCurrencyConversion,
            "T0200" => Self::CurrencyConversion,
            "T1107" => Self::Refund,
            "T2105" => Self::PaymentReviewHold,
            "T2106" => Self::PaymentReviewRelease,
            "T1110" => Self::DisputeHold,
            "T1111" => Self::DisputeHoldCancellation,
            "T1106" => Self::PaymentReversal,
            "T0114" => Self::DisputeFee,
            "T0000" => Self::GeneralPayment,
            "T0011" => Self::MobilePayment,
            "T0006" => Self::ExpressCheckoutPayment,
            _ => Self::Other(value),
        }
    }
}

impl From<TransactionEvent> for String {
    fn from(value: TransactionEvent) -> Self {
        value.as_str().to_owned()
    }
}

#[cfg(test)]
#[path = "../tests/models/paypal.rs"]
mod tests;
