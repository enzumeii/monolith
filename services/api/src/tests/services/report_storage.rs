#![allow(clippy::unwrap_used)]
use super::*;

#[tokio::test]
async fn staged_published_and_recovered_files_keep_identical_bytes() {
    let root = std::env::temp_dir().join(format!("report-storage-{}", Uuid::new_v4()));
    let id = Uuid::new_v4();
    let generated_at = chrono::Utc::now().naive_utc();
    let bytes = b"sku,title\r\n42,original\r\n".to_vec();
    let prepared = prepare(
        &root,
        id,
        generated_at,
        ReportFileFormat::Csv,
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: bytes.clone(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(prepared.files[0].bytes, bytes);
    assert!(!root.join(id.to_string()).exists());
    let files = publish(&root, id).await.unwrap();
    assert_eq!(
        read_published(&root, id, &files[0]).await.unwrap().bytes,
        bytes
    );
    let again = prepare(
        &root,
        id,
        chrono::Utc::now().naive_utc(),
        ReportFileFormat::Csv,
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: b"replacement".to_vec(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(again.manifest.generated_at, generated_at);
    assert_eq!(again.files[0].bytes, bytes);
    assert_eq!(
        publish(&root, id).await.unwrap()[0].storage_key,
        files[0].storage_key
    );
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn rejects_unknown_paths_corruption_and_partial_directories() {
    let root = std::env::temp_dir().join(format!("report-storage-{}", Uuid::new_v4()));
    let id = Uuid::new_v4();
    assert!(
        prepare(
            &root,
            id,
            chrono::Utc::now().naive_utc(),
            ReportFileFormat::Csv,
            vec![GeneratedFile {
                filename: "../escape.csv".into(),
                content_type: CSV_CONTENT_TYPE.into(),
                bytes: vec![]
            }]
        )
        .await
        .is_err()
    );
    let prepared = prepare(
        &root,
        id,
        chrono::Utc::now().naive_utc(),
        ReportFileFormat::Csv,
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: vec![1, 2, 3],
        }],
    )
    .await
    .unwrap();
    assert_eq!(prepared.files[0].bytes, vec![1, 2, 3]);
    fs::write(pending(&root, id).join("inventory.csv"), b"corrupt")
        .await
        .unwrap();
    assert!(publish(&root, id).await.is_err());
    fs::remove_dir_all(root).await.unwrap();
}
