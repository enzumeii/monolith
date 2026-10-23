use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaypalPaymentRequest {
    pub idempotency_key: String,
    pub reference: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaypalPaymentStatus {
    Creating,
    AwaitingApproval,
    Pending,
    Completed,
    Denied,
    Exception,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PaypalPaymentResponse {
    pub id: String,
    pub status: PaypalPaymentStatus,
    pub approval_url: Option<String>,
    pub paypal_order_id: Option<String>,
    pub reference: String,
    pub amount: String,
    pub currency: String,
    pub capture_id: Option<String>,
    pub exception: Option<String>,
}

#[cfg(test)]
#[path = "../tests/dto/paypal_payments.rs"]
mod tests;
