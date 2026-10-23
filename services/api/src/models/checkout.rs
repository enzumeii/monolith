use chrono::{DateTime, Utc};
use domain::dto::checkout::CheckoutMoney;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Checkout {
    // The client knows this identity before the first response, including after a lost response.
    pub id: Uuid,
    pub guest_secret_hash: Vec<u8>,
    pub owner_expires_at: DateTime<Utc>,
    pub request_fingerprint: Vec<u8>,
    pub provider: String,
    pub snapshot: Json<CheckoutSnapshot>,
    pub provider_context: Json<CheckoutProviderContext>,
    pub payment_state: PaymentState,
    pub draft_state: DraftState,
    pub completion_state: CompletionState,
    pub draft_id: Option<String>,
    pub provider_resource_id: Option<String>,
    pub payment_reference: Option<String>,
    pub order_id: Option<String>,
    pub order_name: Option<String>,
    pub redirect_url: Option<String>,
    pub reservation_expires_at: Option<DateTime<Utc>>,
    pub create_request_id: String,
    pub capture_request_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct CheckoutSnapshot {
    pub convention_id: Uuid,
    pub convention_slug: String,
    pub convention_name: String,
    pub email: String,
    pub money: CheckoutMoney,
    pub items: Vec<CheckoutItemSnapshot>,
}

impl std::fmt::Debug for CheckoutSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckoutSnapshot")
            .field("convention_id", &self.convention_id)
            .field("email", &"[REDACTED]")
            .field("money", &self.money)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CheckoutItemSnapshot {
    pub variant_id: Uuid,
    pub shopify_product_id: String,
    pub shopify_variant_id: String,
    pub product_title: Option<String>,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub quantity: u32,
    pub unit_price_minor: i64,
    pub total_minor: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct CheckoutProviderContext {
    pub account_id: String,
    pub environment: String,
    pub correlation_id: String,
}

impl std::fmt::Debug for CheckoutProviderContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckoutProviderContext")
            .field("account_id", &"[REDACTED]")
            .field("environment", &self.environment)
            .finish_non_exhaustive()
    }
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
