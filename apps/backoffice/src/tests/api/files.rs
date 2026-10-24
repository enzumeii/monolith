#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::api::tests::metadata_response;

#[tokio::test]
async fn uploads_raw_image_to_client_folder_with_cookie_transport() {
    let body = r#"{"location":"conventions/event/key.png","url":"https://storage.test/key.png?signature=one","expires_at":"2026-11-01T00:05:00Z"}"#;
    let bytes = b"PNG-image".to_vec();
    let (result, request) = metadata_response(
        201,
        body,
        upload("conventions/event", "image/png", bytes.clone()),
    )
    .await;
    let value = result.expect("upload success");
    assert_eq!(value.location, "conventions/event/key.png");
    assert!(request.starts_with("POST /v1/admin/files?location=conventions%2Fevent HTTP/1.1"));
    assert!(request.contains("content-type: image/png"));
    assert!(request.contains("x-session-mode: cookie"));
    assert!(request.as_bytes().ends_with(&bytes));
    assert!(!request.contains("multipart"));
    let (result, _) = metadata_response(
        413,
        r#"{"error":"Image exceeds 2 MiB"}"#,
        upload("conventions/event", "image/png", vec![0]),
    )
    .await;
    assert_eq!(
        result.expect_err("rejection").to_string(),
        "Image exceeds 2 MiB"
    );
}
