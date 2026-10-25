use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, sqlx::FromRow)]
pub(crate) struct Checkout {
    pub id: Uuid,
    pub completion_version: i16,
    pub correlation_version: i16,
    pub convention_slug: Option<String>,
    pub payment_recording_state: PaymentRecordingState,
    pub guest_secret_hash: Vec<u8>,
    pub owner_expires_at: DateTime<Utc>,
    pub request_fingerprint: Vec<u8>,
    pub convention_id: Uuid,
    pub provider: String,
    pub provider_account_id: String,
    pub provider_environment: String,
    pub evidence_hash: Vec<u8>,
    pub required_tag_hashes: Vec<Vec<u8>>,
    pub success_url: Option<String>,
    pub cancel_url: Option<String>,
    pub gateway_request_fingerprint: Option<Vec<u8>>,
    pub payment_state: PaymentState,
    pub draft_state: DraftState,
    pub completion_state: CompletionState,
    pub draft_id: Option<String>,
    pub provider_resource_id: Option<String>,
    pub order_id: Option<String>,
    pub reservation_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub(crate) enum PaymentState {
    NotStarted,
    Creating,
    RequiresAction,
    Processing,
    Unpaid,
    Paid,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub(crate) enum DraftState {
    NotStarted,
    Creating,
    Reserved,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub(crate) enum CompletionState {
    NotStarted,
    Completing,
    Completed,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub(crate) enum RecoveryCode {
    Observed,
    WaitingForPayment,
    TransientFailure,
    ClaimAbandoned,
    AttemptsExhausted,
    EvidenceConflict,
    MissingPurchase,
    ReservationUnsafe,
    MutationWindowExpired,
    DraftAmbiguous,
    CompletionUncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub(crate) enum PaymentRecordingState {
    NotStarted,
    Recording,
    Uncertain,
    Recorded,
}
#[derive(Clone, Copy)]
pub(crate) struct StripeAuthentication<'a> {
    pub signature: &'a str,
    pub secret: &'a str,
}

pub(crate) struct PaypalAuthentication {
    pub auth_algo: String,
    pub cert_url: String,
    pub transmission_id: String,
    pub transmission_sig: String,
    pub transmission_time: String,
    pub webhook_id: String,
    pub webhook_event: Box<serde_json::value::RawValue>,
}

pub(crate) enum WebhookInput<'a> {
    Stripe {
        raw: &'a [u8],
        authentication: StripeAuthentication<'a>,
    },
    Paypal(crate::models::paypal::VerifyWebhookSignatureRequest),
}
