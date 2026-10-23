use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Clone, FromRow)]
pub(crate) struct PaypalPayment {
    pub id: Uuid,
    pub capability_digest: Vec<u8>,
    pub reference: String,
    pub amount: String,
    pub currency: String,
    pub minor_units: i64,
    pub merchant_id: String,
    pub environment: String,
    pub api_origin: String,
    pub paypal_order_id: Option<String>,
    pub approval_url: Option<String>,
    pub status: String,
    pub create_request_id: Uuid,
    pub capture_request_id: Uuid,
    pub lease_token: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub create_started_at: Option<DateTime<Utc>>,
    pub capture_started_at: Option<DateTime<Utc>>,
    pub verified_capture_id: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub exception: Option<String>,
}

pub(crate) struct NewPaypalPayment {
    pub id: Uuid,
    pub idempotency_key: Uuid,
    pub capability_digest: [u8; 32],
    pub reference: String,
    pub amount: String,
    pub currency: String,
    pub minor_units: i64,
    pub merchant_id: String,
    pub environment: String,
    pub api_origin: String,
    pub create_request_id: Uuid,
    pub capture_request_id: Uuid,
}

#[derive(Clone, Copy)]
pub(crate) enum PaymentOperation {
    Create,
    Capture,
    Reconcile,
}
impl PaymentOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Capture => "CAPTURE",
            Self::Reconcile => "RECONCILE",
        }
    }
}

#[derive(Clone, Debug, FromRow, PartialEq, Eq)]
pub(crate) struct CaptureEvidence {
    pub capture_id: String,
    pub payment_id: Uuid,
    pub merchant_id: String,
    pub environment: String,
    pub paypal_order_id: String,
    pub amount: String,
    pub currency: String,
    pub minor_units: i64,
    pub provider_status: String,
    pub verified_at: DateTime<Utc>,
}

#[derive(Clone, FromRow)]
pub(crate) struct PaypalPaymentEvent {
    pub payload_digest: Vec<u8>,
    pub state: String,
}

pub(crate) struct NewPaypalPaymentEvent {
    pub merchant_id: String,
    pub environment: String,
    pub event_id: String,
    pub payload_digest: [u8; 32],
    pub event_type: String,
    pub paypal_order_id: Option<String>,
    pub capture_id: Option<String>,
    pub verified_at: DateTime<Utc>,
}
