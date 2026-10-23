#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

#[test]
fn refund_resource_id_is_not_used_as_original_capture_identity() {
    let mut event =
        json!({"id":"event","event_type":"PAYMENT.CAPTURE.REFUNDED","resource":{"id":"REFUND-ID"}});
    let identity = parse_event(&event).unwrap();
    assert!(!identity.relevant);
    assert!(identity.capture_id.is_none());
    event["resource"]["supplementary_data"] =
        json!({"related_ids":{"capture_id":"CAPTURE-ID","order_id":"ORDER-ID"}});
    let identity = parse_event(&event).unwrap();
    assert!(identity.relevant);
    assert_eq!(identity.capture_id.as_deref(), Some("CAPTURE-ID"));
    event["resource"]["supplementary_data"]["related_ids"]["capture_id"] = json!("");
    assert!(parse_event(&event).is_err());
}

#[test]
fn completed_events_require_current_verified_evidence() {
    for kind in [
        WebhookEventType::CaptureCompleted,
        WebhookEventType::OrderCompleted,
    ] {
        assert!(
            require_event_evidence(&kind, super::super::ReconciliationOutcome::ObservedUnpaid)
                .is_err()
        );
        assert!(
            require_event_evidence(&kind, super::super::ReconciliationOutcome::Verified).is_ok()
        );
    }
    assert!(
        require_event_evidence(
            &WebhookEventType::CapturePending,
            super::super::ReconciliationOutcome::ObservedUnpaid
        )
        .is_ok()
    );
}

#[test]
fn busy_reconciliation_cannot_acknowledge_any_event_kind() {
    for kind in [
        WebhookEventType::CaptureCompleted,
        WebhookEventType::CapturePending,
        WebhookEventType::OrderApproved,
    ] {
        assert!(require_event_evidence(&kind, super::super::ReconciliationOutcome::Busy).is_err());
    }
}
