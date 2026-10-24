use super::*;
use serde_json::json;

#[test]
fn correlation_falls_back_only_when_top_level_pointer_is_absent() {
    for (custom_id, expected) in [
        (None, Some("nested-reference")),
        (Some(json!("top-reference")), Some("top-reference")),
        (Some(json!(null)), None),
        (Some(json!(7)), None),
        (Some(json!({})), None),
    ] {
        let mut event = json!({
            "id":"EVENT123", "event_type":"CHECKOUT.ORDER.APPROVED",
            "resource":{"id":"ORDER123", "purchase_units":[{"custom_id":"nested-reference"}]}
        });
        if let Some(custom_id) = custom_id {
            event["resource"]["custom_id"] = custom_id;
        }
        assert_eq!(
            decode(&event)
                .expect("valid event")
                .checkout_reference
                .as_deref(),
            expected
        );
    }
    for resource in [
        json!({"id":"ORDER123"}),
        json!({"id":"ORDER123", "purchase_units":7}),
        json!({"id":"ORDER123", "purchase_units":[{"custom_id":false}]}),
    ] {
        let event =
            json!({"id":"EVENT123", "event_type":"CHECKOUT.ORDER.APPROVED", "resource":resource});
        assert_eq!(
            decode(&event).expect("valid event").checkout_reference,
            None
        );
    }
}
