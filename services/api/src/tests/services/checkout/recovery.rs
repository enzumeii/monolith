use super::*;
use crate::client::{ClientError, ClientErrorKind};

#[test]
fn generic_protocol_is_retryable_but_verified_evidence_conflict_is_manual() {
    let malformed = crate::error::upstream(ClientError::new(ClientErrorKind::Protocol(
        "GraphQL data is missing",
    )));
    assert_eq!(
        classify(&malformed),
        RecoveryOutcome::Retry(RecoveryCode::TransientFailure)
    );
    let mismatch = crate::error::upstream(crate::services::payments::evidence_mismatch());
    assert_eq!(
        classify(&mismatch),
        RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict)
    );
    let shared = super::super::conflict("Safe public conflict").with_source(std::sync::Arc::new(
        crate::services::payments::evidence_mismatch(),
    ));
    assert_eq!(
        classify(&shared),
        RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict)
    );
}

#[test]
fn explicit_business_outcome_preserves_safe_code_without_parsing_messages() {
    let error = manual(
        super::super::conflict("anything"),
        RecoveryCode::MissingPurchase,
    );
    assert_eq!(
        classify(&error),
        RecoveryOutcome::Manual(RecoveryCode::MissingPurchase)
    );
    let ordinary =
        super::super::conflict("Original purchase request is required before draft creation");
    assert_eq!(
        classify(&ordinary),
        RecoveryOutcome::Retry(RecoveryCode::TransientFailure)
    );
}

#[tokio::test]
async fn evidence_marker_keeps_safe_upstream_http_contract() {
    use axum::response::IntoResponse;
    let error = crate::error::upstream(crate::services::payments::evidence_mismatch());
    assert_eq!(
        classify(&error),
        RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict)
    );
    let response = error.into_response();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_GATEWAY);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("safe body");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).expect("json"),
        serde_json::json!({"error":"Upstream request failed"})
    );
}

#[tokio::test]
async fn shopify_missing_payload_retries_while_wrong_identity_requires_manual_review() {
    use crate::{
        client::tests::{Fixture, Reply},
        services::common::{client, id},
    };
    let missing = Fixture::new(vec![Reply::json(serde_json::json!({"data":null}))]).await;
    let Err(error) = client(&missing).checkout_draft(&id("DraftOrder", 1)).await else {
        panic!("missing payload accepted");
    };
    assert_eq!(
        classify(&crate::error::upstream(error)),
        RecoveryOutcome::Retry(RecoveryCode::TransientFailure)
    );
    for resource in ["gid://shopify/DraftOrder/2", "gid://shopify/Order/1"] {
        let row = crate::services::checkout::tests::checkout();
        let mut draft = crate::services::checkout::tests::draft(&row);
        draft["id"] = serde_json::json!(resource);
        let fixture = Fixture::new(vec![Reply::json(
            serde_json::json!({"data":{"draftOrder":draft}}),
        )])
        .await;
        let Err(error) = client(&fixture).checkout_draft(&id("DraftOrder", 1)).await else {
            panic!("wrong identity accepted");
        };
        assert_eq!(
            classify(&crate::error::upstream(error)),
            RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict)
        );
        assert_eq!(fixture.requests().len(), 1);
    }
}
