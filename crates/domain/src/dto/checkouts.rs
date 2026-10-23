use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyPaymentResponse {
    pub payment_state: CheckoutPaymentState,
    pub completion_state: CheckoutCompletionState,
    pub order_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckoutPaymentState {
    Pending,
    Unpaid,
    Expired,
    Paid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckoutCompletionState {
    Ready,
    Processing,
    Uncertain,
    OrderSaved,
    Complete,
    Operator,
}
