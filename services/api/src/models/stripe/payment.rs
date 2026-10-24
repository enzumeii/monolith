use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ExpectedPayment {
    pub session_id: String,
    pub account_id: String,
    pub livemode: bool,
    pub amount: i64,
    pub currency: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaymentOutcome {
    RequiresAction,
    Pending,
    ExpiredUnpaid,
    VerifiedPaid(VerifiedPayment),
}

#[derive(Debug, PartialEq, Eq)]
pub struct VerifiedPayment {
    pub session_id: String,
    pub payment_intent_id: String,
    pub charge_id: String,
    pub amount: i64,
    pub currency: String,
    pub refunded: bool,
    pub amount_refunded: i64,
    pub disputed: bool,
}
