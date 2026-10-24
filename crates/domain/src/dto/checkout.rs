use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutProvider {
    Stripe,
    Paypal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateCheckoutRequest {
    pub operation_id: String,
    pub convention_id: String,
    pub email: String,
    pub provider: CheckoutProvider,
    pub items: Vec<CheckoutItemInput>,
    pub expected_currency: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckoutItemInput {
    pub variant_id: String,
    pub quantity: u32,
    pub expected_unit_price: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CheckoutMoney {
    pub minor: i64,
    pub currency: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutStatus {
    RequiresAction,
    Processing,
    Unpaid,
    ReconciliationRequired,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckoutNextAction {
    Redirect { url: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CheckoutOrder {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CheckoutResponse {
    pub checkout_id: String,
    pub status: CheckoutStatus,
    pub money: CheckoutMoney,
    pub reservation_expires_at: Option<String>,
    pub next_action: Option<CheckoutNextAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<CheckoutOrder>,
}

#[cfg(test)]
#[path = "../tests/dto/checkout.rs"]
mod tests;
