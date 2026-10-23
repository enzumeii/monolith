use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateOrderRequest {
    pub intent: OrderIntent,
    pub purchase_units: Vec<PurchaseUnitRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_source: Option<PaymentSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_context: Option<ExperienceContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderIntent {
    Capture,
    Authorize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PurchaseUnitRequest {
    pub reference_id: String,
    pub custom_id: String,
    pub amount: PaymentMoney,
    pub payee: Payee,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaymentMoney {
    pub currency_code: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Payee {
    pub merchant_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentSource {
    pub paypal: PaypalPaymentSource,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaypalPaymentSource {
    pub experience_context: ExperienceContext,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExperienceContext {
    pub return_url: String,
    pub cancel_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Order {
    pub id: String,
    pub status: OrderStatus,
    #[serde(default)]
    pub purchase_units: Vec<PurchaseUnit>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PurchaseUnit {
    pub reference_id: Option<String>,
    pub custom_id: Option<String>,
    pub amount: PaymentMoney,
    pub payee: Option<Payee>,
    pub payments: Option<PaymentCollection>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentCollection {
    #[serde(default)]
    pub captures: Vec<Capture>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Capture {
    pub id: String,
    pub status: CaptureStatus,
    pub amount: PaymentMoney,
    pub final_capture: Option<bool>,
    pub payee: Option<Payee>,
    pub custom_id: Option<String>,
    pub supplementary_data: Option<SupplementaryData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SupplementaryData {
    pub related_ids: RelatedIds,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RelatedIds {
    pub order_id: Option<String>,
    pub capture_id: Option<String>,
    pub authorization_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Link {
    pub href: String,
    pub rel: String,
    pub method: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct VerifyWebhookSignatureRequest {
    pub auth_algo: String,
    pub cert_url: String,
    pub transmission_id: String,
    pub transmission_sig: String,
    pub transmission_time: String,
    pub webhook_id: String,
    pub webhook_event: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerifyWebhookSignatureResponse {
    pub verification_status: WebhookVerificationStatus,
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name {
            $($variant,)+
            Other(String),
        }

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value,
                }
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value),
                }
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.as_str().to_owned()
            }
        }
    };
}

wire_enum!(OrderStatus {
    Created => "CREATED",
    Saved => "SAVED",
    Approved => "APPROVED",
    Voided => "VOIDED",
    Completed => "COMPLETED",
    PayerActionRequired => "PAYER_ACTION_REQUIRED",
});

wire_enum!(CaptureStatus {
    Completed => "COMPLETED",
    Declined => "DECLINED",
    Denied => "DENIED",
    Pending => "PENDING",
    Failed => "FAILED",
    PartiallyRefunded => "PARTIALLY_REFUNDED",
    Refunded => "REFUNDED",
});

wire_enum!(WebhookVerificationStatus {
    Success => "SUCCESS",
    Failure => "FAILURE",
});

wire_enum!(WebhookEventType {
    CaptureCompleted => "PAYMENT.CAPTURE.COMPLETED",
    CapturePending => "PAYMENT.CAPTURE.PENDING",
    CaptureDenied => "PAYMENT.CAPTURE.DENIED",
    CaptureRefunded => "PAYMENT.CAPTURE.REFUNDED",
    CaptureReversed => "PAYMENT.CAPTURE.REVERSED",
    OrderApproved => "CHECKOUT.ORDER.APPROVED",
    OrderCompleted => "CHECKOUT.ORDER.COMPLETED",
});
