#![allow(clippy::unwrap_used)]
use super::*;
use crate::{blob_fixture::Fixture, models::cron::checkpoint::FileCheckpoint};

fn entry() -> FileCheckpoint {
    FileCheckpoint {
        filename: "inventory.csv".into(),
        key: "inventory/20261022/11111111-1111-4111-8111-111111111111/inventory.csv".into(),
        content_type: report_generator::CSV_CONTENT_TYPE.into(),
        size_bytes: 8,
        sha256: digest(b"original"),
        receipt: None,
    }
}
#[tokio::test]
async fn lost_put_response_is_reconciled_without_reupload_and_recovery_uses_remote_bytes() {
    let fixture = Fixture::new().await;
    fixture.data.lock().unwrap().lost_response = true;
    let entry = entry();
    let (blob, bytes) = obtain(&fixture.client, &entry, Some(b"original"))
        .await
        .unwrap();
    assert_eq!(bytes, b"original");
    assert_eq!(blob.pathname, entry.key);
    let (_, recovered) = obtain(&fixture.client, &entry, None).await.unwrap();
    assert_eq!(recovered, bytes);
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 1);
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
        assert!(obtain(&fixture.client, &entry, None).await.is_err());
    }
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 0);
}
#[tokio::test]
async fn failed_upload_has_one_put_and_malformed_accepted_receipt_recovers_by_key() {
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
async fn saved_receipt_cannot_redirect_to_another_key_or_untrusted_origin() {
    let fixture = Fixture::new().await;
    let mut entry = entry();
    fixture
        .data
        .lock()
        .unwrap()
        .objects
        .insert(entry.key.clone(), b"original".to_vec());
    fixture
        .data
        .lock()
        .unwrap()
        .objects
        .insert("other.csv".into(), b"original".to_vec());
    for url in [
        format!("{}/other.csv", fixture.base),
        "https://untrusted.example.test/inventory.csv".into(),
    ] {
        entry.receipt = Some(crate::models::cron::report::SavedFile {
            filename: entry.filename.clone(),
            content_type: entry.content_type.clone(),
            storage_key: entry.key.clone(),
            download_url: url,
        });
        assert!(obtain(&fixture.client, &entry, None).await.is_err());
    }
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 0);
}
