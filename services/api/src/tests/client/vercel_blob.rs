#![allow(clippy::unwrap_used)]
use super::*;
use crate::blob_fixture::Fixture;

#[tokio::test]
async fn raw_upload_metadata_download_preserve_bytes_and_reject_overwrite() {
    let fixture = Fixture::new().await;
    let key = "inventory/20261022/11111111-1111-4111-8111-111111111111/inventory.csv";
    assert!(fixture.client.metadata(key).await.unwrap().is_none());
    let blob = fixture
        .client
        .put(key, "text/csv; charset=utf-8", b"original".to_vec())
        .await
        .unwrap();
    assert_eq!(
        fixture
            .client
            .download(&blob.download_url, 8)
            .await
            .unwrap(),
        b"original"
    );
    assert_eq!(
        fixture
            .client
            .metadata(key)
            .await
            .unwrap()
            .unwrap()
            .pathname,
        key
    );
    assert!(matches!(
        fixture
            .client
            .put(key, "text/csv; charset=utf-8", b"other".to_vec())
            .await
            .unwrap_err()
            .kind(),
        ClientErrorKind::Http { status: 412, .. }
    ));
    assert_eq!(fixture.data.lock().unwrap().puts.len(), 2);
}
#[test]
fn rejects_unsafe_returned_urls_and_wrong_pathname() {
    let client = VercelBlobClient::new(&VercelBlobConfig {
        api_url: "https://vercel.com/api/blob".into(),
        timeout_secs: 1,
        token: Some("synthetic".into()),
    })
    .unwrap();
    for url in [
        "http://x.public.blob.vercel-storage.com/inventory.csv",
        "https://evil.test/inventory.csv",
        "https://secret@x.public.blob.vercel-storage.com/inventory.csv",
        "https://x.public.blob.vercel-storage.com/inventory.csv#secret",
    ] {
        assert!(client.download_url(url).is_err());
    }
    assert!(
        client
            .download_url("https://store.public.blob.vercel-storage.com/inventory.csv?download=1")
            .is_ok()
    );
    let blob = Blob {
        pathname: "wrong.csv".into(),
        content_type: "text/csv".into(),
        url: "https://store.public.blob.vercel-storage.com/wrong.csv".into(),
        download_url: "https://store.public.blob.vercel-storage.com/wrong.csv".into(),
    };
    assert!(client.validate(&blob, "inventory.csv", "text/csv").is_err());
}

#[tokio::test]
async fn oversized_chunked_download_is_rejected_before_unbounded_allocation() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request).await;
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\n0123456789abcdef\r\n",
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let client = VercelBlobClient::new(&VercelBlobConfig {
        api_url: base.clone(),
        timeout_secs: 1,
        token: Some("synthetic".into()),
    })
    .unwrap();
    let result = client
        .download(&format!("{base}/inventory.csv"), 3)
        .await
        .unwrap_err();
    assert!(matches!(
        result.kind(),
        ClientErrorKind::Protocol("Blob response exceeds expected size")
    ));
    task.abort();
}
#[test]
fn configuration_redacts_tokens_and_rejects_invalid_endpoint_or_timeout() {
    let mut config = VercelBlobConfig {
        api_url: "http://localhost:3001/".into(),
        timeout_secs: 1,
        token: Some("private-fixture-token".into()),
    };
    assert!(!format!("{config:?}").contains("private-fixture-token"));
    config.timeout_secs = 0;
    assert!(VercelBlobClient::new(&config).is_err());
    config.timeout_secs = 1;
    config.api_url = "http://secret@localhost:3001/".into();
    assert!(VercelBlobClient::new(&config).is_err());
    config.api_url = "http://localhost:3001/".into();
    config.token = None;
    assert!(VercelBlobClient::new(&config).is_err());
}
