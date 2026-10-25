#![allow(clippy::unwrap_used)]
use super::*;
use crate::models::checkout::StripeAuthentication;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;

#[tokio::test]
async fn stripe_authentication_and_notification_policy_precede_advancement_dependencies() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost/fixture")
        .unwrap();
    let settings: CheckoutSettings = serde_json::from_value(json!({
        "frontend_origin":"https://example.test",
        "return_path_template":"/{convention_slug}/{checkout_id}",
        "cancel_path_template":"/{convention_slug}/{checkout_id}",
        "shopify_location_id":"1",
        "reservation_secs":3600,"guest_recovery_secs":604_800,"stripe_livemode":false
    }))
    .unwrap();
    for (kind, livemode, account, valid_signature, expected) in [
        ("invoice.created", false, None, true, None),
        (
            "invoice.created",
            false,
            Some("acct_fixture"),
            true,
            Some("Stripe client unavailable"),
        ),
        (
            "invoice.created",
            true,
            Some("acct_fixture"),
            true,
            Some("Stripe webhook account or environment mismatch"),
        ),
        (
            "checkout.session.completed",
            false,
            None,
            false,
            Some("Invalid Stripe webhook authentication or event"),
        ),
        (
            "checkout.session.completed",
            false,
            None,
            true,
            Some("Shopify client unavailable"),
        ),
    ] {
        let raw = json!({"id":"evt_fixture", "type":kind, "livemode":livemode, "account":account,
            "data":{"object":{"id":"cs_fixture", "object":"checkout.session"}}})
        .to_string();
        let timestamp = chrono::Utc::now().timestamp();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"whsec_fixture").unwrap();
        mac.update(format!("{timestamp}.{raw}").as_bytes());
        let signature = if valid_signature {
            hex::encode(mac.finalize().into_bytes())
        } else {
            "00".repeat(32)
        };
        let header = format!("t={timestamp},v1={signature}");
        let clients = WebhookClients {
            stripe: Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Stripe client unavailable",
            )),
            paypal: Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "PayPal client unavailable",
            )),
            advancement: Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shopify client unavailable",
            )),
        };
        let result = receive(
            &pool,
            &settings,
            clients,
            WebhookInput::Stripe {
                raw: raw.as_bytes(),
                authentication: StripeAuthentication {
                    signature: &header,
                    secret: "whsec_fixture",
                },
            },
        )
        .await;
        assert_eq!(
            result.err().map(|error| error.to_string()).as_deref(),
            expected,
            "{kind}, {account:?}, live={livemode}"
        );
    }
}
