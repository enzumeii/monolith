use super::*;
const RAW: &[u8] = br#"{"id":"evt_test","type":"checkout.session.completed","livemode":false,"account":"acct_test","data":{"object":{"id":"cs_test","object":"checkout.session","payment_intent":"pi_test"}}}"#;

#[test]
fn authenticated_incomplete_or_wrong_type_events_fail_closed() {
    for payload in [
        "{}",
        r#"{"id":"evt_test","type":"checkout.session.completed","livemode":false,"data":{"object":{}}}"#,
        r#"{"id":"","type":"checkout.session.completed","livemode":false,"data":{"object":{"id":"cs_x","object":"checkout.session"}}}"#,
        r#"{"id":"evt_test","type":"","livemode":false,"data":{"object":{"id":"cs_x","object":"checkout.session"}}}"#,
        r#"{"id":"evt_test","type":"checkout.session.completed","livemode":"false","data":{"object":{"id":"cs_x","object":"checkout.session"}}}"#,
        r#"{"id":"evt_test","type":"checkout.session.completed","livemode":false,"data":{"object":{"id":"cs_x","object":"checkout.session","payment_intent":{}}}}"#,
    ] {
        assert_eq!(
            VerifiedEvent::decode(payload.as_bytes()),
            Err(WebhookError::InvalidEvent)
        );
    }
}

#[test]
fn preserves_unknown_event_identity_and_expanded_object_references() {
    let payload = br#"{"id":"evt_future","type":"future.payment.signal","api_version":"2026-09-30.clover","livemode":true,"data":{"object":{"id":"obj_future","object":"future.object","payment_intent":{"id":"pi_expanded","object":"payment_intent"},"latest_charge":"ch_real"}}}"#;
    let event = VerifiedEvent::decode(payload).expect("unknown event authenticates");
    assert_eq!(
        event.event_type,
        EventType::Other("future.payment.signal".into())
    );
    assert_eq!(event.event_type.as_str(), "future.payment.signal");
    assert_eq!(event.account, None);
    assert!(event.livemode);
    assert_eq!(event.object_id, "obj_future");
}

#[test]
fn authenticated_events_preserve_identity_version_and_reference_shape_validation() {
    use serde_json::{Value, json};

    let original: Value = serde_json::from_slice(RAW).expect("fixture JSON");
    let mut payloads = Vec::new();
    let mut missing_id = original.clone();
    missing_id
        .as_object_mut()
        .expect("event object")
        .remove("id");
    payloads.push(missing_id);
    for id in [
        Value::Null,
        json!(7),
        json!("evt/invalid"),
        json!("a".repeat(256)),
    ] {
        let mut payload = original.clone();
        payload["id"] = id;
        payloads.push(payload);
    }
    for version in [json!(7), json!(false), json!({}), json!([])] {
        let mut payload = original.clone();
        payload["api_version"] = version;
        payloads.push(payload);
    }
    for field in ["payment_intent", "latest_charge"] {
        for reference in [json!("pi/invalid"), json!({"id":"ch/invalid"}), json!({})] {
            let mut payload = original.clone();
            payload["data"]["object"][field] = reference;
            payloads.push(payload);
        }
    }
    for payload in payloads {
        let raw = serde_json::to_vec(&payload).expect("encode fixture");
        assert_eq!(
            VerifiedEvent::decode(&raw),
            Err(WebhookError::InvalidEvent),
            "{payload}"
        );
    }
}

#[test]
fn correlation_is_optional_text_and_malformed_metadata_remains_tolerated() {
    use serde_json::{Value, json};
    let original: Value = serde_json::from_slice(RAW).expect("event fixture");
    for (metadata, expected) in [
        (
            json!({"checkout_id":"checkout-reference"}),
            Some("checkout-reference"),
        ),
        (json!({"checkout_id":7}), None),
        (json!({"checkout_id":null}), None),
        (json!({}), None),
        (json!(7), None),
        (json!(null), None),
        (json!([]), None),
    ] {
        let mut event = original.clone();
        event["data"]["object"]["metadata"] = metadata;
        let raw = serde_json::to_vec(&event).expect("encode fixture");
        assert_eq!(
            VerifiedEvent::decode(&raw)
                .expect("event remains valid")
                .checkout_reference
                .as_deref(),
            expected
        );
    }
}

#[test]
fn repeated_metadata_preserves_last_value_without_rejecting_authenticated_event() {
    let raw = br#"{"id":"evt_test","type":"checkout.session.completed","livemode":false,"data":{"object":{"id":"cs_test","object":"checkout.session","metadata":{"checkout_id":"first"},"metadata":{"checkout_id":"last"}}}}"#;
    assert_eq!(
        VerifiedEvent::decode(raw)
            .expect("duplicate ignored field remains valid")
            .checkout_reference
            .as_deref(),
        Some("last")
    );
}
