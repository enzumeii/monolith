use super::*;

const RAW: &[u8] = br#"{"id":"evt_test","type":"checkout.session.completed","livemode":false,"account":"acct_test","data":{"object":{"id":"cs_test","object":"checkout.session","payment_intent":"pi_test"}}}"#;
// Independently generated with Python's standard-library HMAC-SHA256.
const SIGNATURE: &str = "a1002766a9b73e803fc6756fa934f8812cd4af82312bc85e4afdddd41d19fd07";
const SECRET: &str = "whsec_synthetic";
const NOW: i64 = 1_700_000_000;

#[test]
fn authenticates_independently_computed_signature() {
    let event =
        verify_event(RAW, &header(), SECRET, NOW, Duration::from_mins(5)).expect("valid event");
    assert_eq!(event.id, "evt_test");
    assert_eq!(event.event_type, EventType::CheckoutSessionCompleted);
    assert!(!event.livemode);
    assert_eq!(event.account.as_deref(), Some("acct_test"));
    assert_eq!(event.object.id, "cs_test");
    assert_eq!(event.object.object_type, "checkout.session");
    assert_eq!(
        event.object.payment_intent.expect("reference").id(),
        "pi_test"
    );
}

#[test]
fn raw_byte_mutations_and_wrong_secret_fail_authentication() {
    let mut raw = RAW.to_vec();
    raw.push(b' ');
    for (raw, secret) in [(raw.as_slice(), SECRET), (RAW, "whsec_wrong")] {
        assert_eq!(
            verify_event(raw, &header(), secret, NOW, Duration::from_mins(5)),
            Err(WebhookError::InvalidSignature)
        );
    }
}

#[test]
fn rotation_accepts_any_matching_v1_signature() {
    let wrong = "00".repeat(32);
    for signatures in [
        format!("v1={wrong},v1={SIGNATURE}"),
        format!("v1={SIGNATURE},v1={wrong}"),
    ] {
        assert!(
            verify_event(
                RAW,
                &format!("t={NOW},v0=ignored,{signatures}"),
                SECRET,
                NOW,
                Duration::from_mins(5)
            )
            .is_ok()
        );
    }
}

#[test]
fn malformed_or_ambiguous_headers_are_rejected() {
    for header in [
        String::new(),
        format!("v1={SIGNATURE}"),
        format!("t={NOW}"),
        format!("t={NOW},t={NOW},v1={SIGNATURE}"),
        format!("t={NOW},t={},v1={SIGNATURE}", NOW + 1),
        format!("t=-1,v1={SIGNATURE}"),
        format!("t=+{NOW},v1={SIGNATURE}"),
        format!("t=99999999999999999999999999,v1={SIGNATURE}"),
        format!("t={NOW},v1=zz{}", "00".repeat(31)),
        format!("t={NOW},v1={}", &SIGNATURE[..62]),
        format!("t={NOW},v1={SIGNATURE},v1=bad"),
        format!("t={NOW},v1={SIGNATURE},bad"),
    ] {
        assert_eq!(
            verify_event(RAW, &header, SECRET, NOW, Duration::from_mins(5)),
            Err(WebhookError::InvalidHeader)
        );
    }
}

#[test]
fn timestamp_tolerance_accepts_boundaries_and_rejects_stale_or_future() {
    for now in [NOW - 300, NOW, NOW + 300] {
        assert!(verify_event(RAW, &header(), SECRET, now, Duration::from_mins(5)).is_ok());
    }
    for now in [NOW - 301, NOW + 301, i64::MAX] {
        assert_eq!(
            verify_event(RAW, &header(), SECRET, now, Duration::from_mins(5)),
            Err(WebhookError::TimestampOutsideTolerance)
        );
    }
}

#[test]
fn invalid_configuration_never_disables_timestamp_verification() {
    for tolerance in [
        Duration::ZERO,
        Duration::from_millis(1),
        Duration::from_secs(901),
    ] {
        assert_eq!(
            verify_event(RAW, &header(), SECRET, NOW, tolerance),
            Err(WebhookError::InvalidConfiguration)
        );
    }
    for (secret, now) in [("", NOW), (SECRET, -1)] {
        assert_eq!(
            verify_event(RAW, &header(), secret, now, Duration::from_mins(5)),
            Err(WebhookError::InvalidConfiguration)
        );
    }
}

#[test]
fn authentication_precedes_json_decoding() {
    assert_eq!(
        verify_event(
            b"invalid json",
            &header(),
            SECRET,
            NOW,
            Duration::from_mins(5)
        ),
        Err(WebhookError::InvalidSignature)
    );
    assert_eq!(
        verify_event(
            b"invalid json",
            &sign(b"invalid json"),
            SECRET,
            NOW,
            Duration::from_mins(5)
        ),
        Err(WebhookError::InvalidEvent)
    );
}

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
            verify_event(
                payload.as_bytes(),
                &sign(payload.as_bytes()),
                SECRET,
                NOW,
                Duration::from_mins(5)
            ),
            Err(WebhookError::InvalidEvent)
        );
    }
}

#[test]
fn preserves_unknown_event_identity_and_expanded_object_references() {
    let payload = br#"{"id":"evt_future","type":"future.payment.signal","api_version":"2026-09-30.clover","livemode":true,"data":{"object":{"id":"obj_future","object":"future.object","payment_intent":{"id":"pi_expanded","object":"payment_intent"},"latest_charge":"ch_real"}}}"#;
    let event = verify_event(payload, &sign(payload), SECRET, NOW, Duration::from_mins(5))
        .expect("unknown event authenticates");
    assert_eq!(
        event.event_type,
        EventType::Other("future.payment.signal".into())
    );
    assert_eq!(event.event_type.as_str(), "future.payment.signal");
    assert_eq!(event.api_version.as_deref(), Some("2026-09-30.clover"));
    assert_eq!(event.account, None);
    assert!(event.livemode);
    assert_eq!(
        event.object.payment_intent.expect("expanded").id(),
        "pi_expanded"
    );
    assert_eq!(event.object.latest_charge.expect("charge").id(), "ch_real");
}

#[test]
fn bounds_body_header_and_signature_count() {
    assert_eq!(
        verify_event(
            &vec![b' '; MAX_BODY_BYTES + 1],
            &header(),
            SECRET,
            NOW,
            Duration::from_mins(5)
        ),
        Err(WebhookError::InputTooLarge)
    );
    assert_eq!(
        verify_event(
            RAW,
            &" ".repeat(MAX_HEADER_BYTES + 1),
            SECRET,
            NOW,
            Duration::from_mins(5)
        ),
        Err(WebhookError::InputTooLarge)
    );
    let repeated = format!("t={NOW},{}", format!("v1={SIGNATURE},").repeat(64));
    assert_eq!(
        verify_event(RAW, &repeated, SECRET, NOW, Duration::from_mins(5)),
        Err(WebhookError::InvalidHeader)
    );
}

fn header() -> String {
    format!("t={NOW},v1={SIGNATURE}")
}

fn sign(raw: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).expect("valid HMAC key");
    mac.update(format!("{NOW}.").as_bytes());
    mac.update(raw);
    format!("t={NOW},v1={}", hex::encode(mac.finalize().into_bytes()))
}
