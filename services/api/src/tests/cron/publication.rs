#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    models::cron::checkpoint::FileCheckpoint,
    s3_fixture::{Fixture, PutGate},
};

fn entry() -> FileCheckpoint {
    FileCheckpoint {
        filename: "inventory.csv".into(),
        key: format!(
            "inventory/20261022/11111111-1111-4111-8111-111111111111/{}/inventory.csv",
            digest(b"original")
        ),
        content_type: report_generator::CSV_CONTENT_TYPE.into(),
        size_bytes: 8,
        sha256: digest(b"original"),
        receipt: None,
    }
}
fn receipt(entry: &FileCheckpoint) -> SavedFile {
    SavedFile {
        filename: entry.filename.clone(),
        content_type: entry.content_type.clone(),
        bucket: "reports".into(),
        storage_key: entry.key.clone(),
        size_bytes: entry.size_bytes,
        sha256: entry.sha256.clone(),
    }
}
#[tokio::test]
async fn lost_put_response_is_reconciled_without_reupload_and_recovery_uses_remote_bytes() {
    let fixture = Fixture::new().await;
    fixture.data.lock().unwrap().lost_response = true;
    let mut entry = entry();
    let (saved, bytes) = obtain(&fixture.client, &entry, Some(b"original"))
        .await
        .unwrap();
    assert_eq!(bytes, b"original");
    assert_eq!(saved, receipt(&entry));
    entry.receipt = Some(saved.clone());
    let (recovered_receipt, recovered) = obtain(&fixture.client, &entry, None).await.unwrap();
    assert_eq!(recovered_receipt, saved);
    assert_eq!(recovered, bytes);
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 1);
    assert_eq!(
        fixture.data.lock().unwrap().calls,
        [
            "metadata", "upload", "metadata", "download", "metadata", "download"
        ]
    );
    let json = serde_json::to_value(saved).unwrap();
    assert!(json.get("download_url").is_none());
}
#[tokio::test]
async fn confirmed_missing_corrupt_and_oversized_objects_fail_without_put() {
    let fixture = Fixture::new().await;
    let entry = entry();
    assert!(obtain(&fixture.client, &entry, None).await.is_err());
    for bytes in [
        b"changed!".to_vec(),
        b"original-more".to_vec(),
        b"short".to_vec(),
    ] {
        fixture
            .data
            .lock()
            .unwrap()
            .objects
            .insert(entry.key.clone(), bytes);
        assert!(
            obtain(&fixture.client, &entry, Some(b"original"))
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 0);
}
#[tokio::test]
async fn failed_upload_has_one_put_and_malformed_accepted_response_recovers_by_key() {
    let fixture = Fixture::new().await;
    let entry = entry();
    fixture.data.lock().unwrap().fail_key = Some(entry.key.clone());
    assert!(
        obtain(&fixture.client, &entry, Some(b"original"))
            .await
            .is_err()
    );
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 1);
    fixture.data.lock().unwrap().fail_key = None;
    fixture.data.lock().unwrap().malformed = true;
    assert!(
        obtain(&fixture.client, &entry, Some(b"original"))
            .await
            .is_ok()
    );
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 2);
}
#[tokio::test]
async fn saved_receipt_cannot_redirect_to_another_bucket_key_or_digest() {
    let fixture = Fixture::new().await;
    for mutation in ["bucket", "key", "digest", "filename", "size", "type"] {
        let mut entry = entry();
        let mut saved = receipt(&entry);
        match mutation {
            "bucket" => saved.bucket = "other-reports".into(),
            "key" => saved.storage_key = saved.storage_key.replace("11111111", "22222222"),
            "digest" => saved.sha256 = "a".repeat(64),
            "filename" => saved.filename = "other.csv".into(),
            "size" => saved.size_bytes += 1,
            _ => saved.content_type = "text/plain".into(),
        }
        entry.receipt = Some(saved);
        assert!(
            obtain(&fixture.client, &entry, None).await.is_err(),
            "{mutation}"
        );
    }
    assert_eq!(
        fixture.data.lock().unwrap().calls,
        [] as [std::string::String; 0]
    );
}
#[tokio::test]
async fn mutated_original_body_or_key_is_rejected_before_network_access() {
    let fixture = Fixture::new().await;
    for bytes in [b"changed!".as_slice(), b"original-more", b"short"] {
        assert!(
            obtain(&fixture.client, &entry(), Some(bytes))
                .await
                .is_err()
        );
    }
    let mut entry = entry();
    entry.sha256 = "a".repeat(64);
    assert!(
        obtain(&fixture.client, &entry, Some(b"original"))
            .await
            .is_err()
    );
    assert_eq!(
        fixture.data.lock().unwrap().calls,
        [] as [std::string::String; 0]
    );
}
#[tokio::test]
async fn metadata_and_download_must_both_match_the_frozen_file() {
    let fixture = Fixture::new().await;
    let entry = entry();
    fixture
        .data
        .lock()
        .unwrap()
        .objects
        .insert(entry.key.clone(), b"original".to_vec());
    fixture.data.lock().unwrap().content_type = Some("text/plain".into());
    assert!(obtain(&fixture.client, &entry, None).await.is_err());
    assert_eq!(fixture.data.lock().unwrap().calls, ["metadata"]);
    fixture.data.lock().unwrap().content_type = None;
    for bytes in [
        b"changed!".to_vec(),
        b"short".to_vec(),
        b"original-more".to_vec(),
    ] {
        fixture.data.lock().unwrap().download_override = Some(bytes);
        assert!(obtain(&fixture.client, &entry, None).await.is_err());
    }
    assert_eq!(
        fixture.data.lock().unwrap().puts,
        [] as [std::string::String; 0]
    );
}
#[tokio::test]
async fn an_existing_receipt_never_authorizes_recreating_a_missing_object() {
    let fixture = Fixture::new().await;
    let mut entry = entry();
    entry.receipt = Some(receipt(&entry));
    assert!(
        obtain(&fixture.client, &entry, Some(b"original"))
            .await
            .is_err()
    );
    assert_eq!(
        fixture.data.lock().unwrap().puts,
        [] as [std::string::String; 0]
    );
}
#[tokio::test]
async fn concurrent_upserts_only_replay_identical_frozen_bytes() {
    let fixture = Fixture::new().await;
    let gate = std::sync::Arc::new(PutGate::default());
    fixture.data.lock().unwrap().put_gate = Some(gate.clone());
    let entry = entry();
    let run = async {
        let a = obtain(&fixture.client, &entry, Some(b"original"));
        let b = obtain(&fixture.client, &entry, Some(b"original"));
        let release = async {
            gate.started.acquire_many(2).await.unwrap().forget();
            gate.release.add_permits(2);
        };
        let (a, b, ()) = tokio::join!(a, b, release);
        assert_eq!(a.unwrap(), b.unwrap());
    };
    tokio::time::timeout(std::time::Duration::from_secs(3), Box::pin(run))
        .await
        .unwrap();
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 2);
    assert_eq!(
        fixture.data.lock().unwrap().objects[&entry.key],
        b"original"
    );
    fixture
        .data
        .lock()
        .unwrap()
        .objects
        .insert(entry.key.clone(), b"changed!".to_vec());
    assert!(
        obtain(&fixture.client, &entry, Some(b"original"))
            .await
            .is_err()
    );
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 2);
}
