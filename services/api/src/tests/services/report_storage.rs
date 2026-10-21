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
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: bytes.clone(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(prepared.files[0].bytes, bytes);
    assert!(!root.join("public").join(id.to_string()).exists());
    let files = publish(
        &ReportStorageConfig::Local {
            directory: root.clone(),
            public_base_url: "http://localhost:3001/".into(),
        },
        id,
    )
    .await
    .unwrap();
    assert_eq!(
        load(&root, id).await.unwrap().unwrap().files[0].bytes,
        bytes
    );
    let again = prepare(
        &root,
        id,
        chrono::Utc::now().naive_utc(),
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
        publish(
            &ReportStorageConfig::Local {
                directory: root.clone(),
                public_base_url: "http://localhost:3001/".into()
            },
            id
        )
        .await
        .unwrap()[0]
            .storage_key,
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
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: vec![1, 2, 3],
        }],
    )
    .await
    .unwrap();
    assert_eq!(prepared.files[0].bytes, vec![1, 2, 3]);
    fs::write(pending(&root, id).join("files/inventory.csv"), b"corrupt")
        .await
        .unwrap();
    assert!(
        publish(
            &ReportStorageConfig::Local {
                directory: root.clone(),
                public_base_url: "http://localhost:3001/".into()
            },
            id
        )
        .await
        .is_err()
    );
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn email_request_is_immutable_and_acceptance_is_private() {
    let root = std::env::temp_dir().join(format!("mail-checkpoint-{}", Uuid::new_v4()));
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().naive_utc();
    prepare(
        &root,
        id,
        now,
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: b"original".to_vec(),
        }],
    )
    .await
    .unwrap();
    let first = MailIntent {
        from: "sender@example.test".into(),
        to: "first@example.test".into(),
        subject: "Report".into(),
        html: "Report".into(),
        first_attempt_at: now,
    };
    mail_intent(&root, id, &first).await.unwrap();
    let mut changed = first.clone();
    changed.to = "changed@example.test".into();
    assert_eq!(mail_intent(&root, id, &changed).await.unwrap().to, first.to);
    assert!(!accepted(&root, id).await.unwrap());
    accept(&root, id).await.unwrap();
    accept(&root, id).await.unwrap();
    assert!(accepted(&root, id).await.unwrap());
    let storage = ReportStorageConfig::Local {
        directory: root.clone(),
        public_base_url: "http://localhost:3001/files".into(),
    };
    let files = publish(&storage, id).await.unwrap();
    assert_eq!(
        files[0].download_url,
        format!("http://localhost:3001/files/{id}/inventory.csv")
    );
    assert!(
        !root
            .join("public")
            .join(id.to_string())
            .join("manifest.json")
            .exists()
    );
    assert!(
        !root
            .join("public")
            .join(id.to_string())
            .join("email.json")
            .exists()
    );
    fs::remove_dir_all(root).await.unwrap();
}

#[tokio::test]
async fn missing_manifest_and_corrupt_acceptance_fail_without_reconstruction() {
    let root = std::env::temp_dir().join(format!("report-partial-{}", Uuid::new_v4()));
    let id = Uuid::new_v4();
    fs::create_dir_all(pending(&root, id)).await.unwrap();
    assert!(load(&root, id).await.is_err());
    fs::write(pending(&root, id).join("accepted"), b"partial")
        .await
        .unwrap();
    assert!(accepted(&root, id).await.is_err());
    fs::remove_dir_all(root).await.unwrap();
}
