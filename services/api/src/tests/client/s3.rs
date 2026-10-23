#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn config(endpoint: &str) -> S3Config {
    S3Config {
        endpoint: endpoint.into(),
        region: "ap-southeast-1".into(),
        bucket: "reports".into(),
        force_path_style: true,
        timeout_secs: 1,
        signed_url_ttl_secs: 300,
        access_key_id: Some("synthetic-access-id".into()),
        secret_access_key: Some("synthetic-secret-key".into()),
    }
}
fn client(f: &Fixture) -> S3Client {
    S3Client::new(&config(f.url.as_str())).unwrap()
}
fn ok(body: &str) -> Reply {
    Reply {
        status: 200,
        body: body.into(),
        headers: vec![("content-type".into(), "text/csv; charset=utf-8".into())],
    }
}
#[tokio::test]
async fn metadata_put_download_use_signed_requests_and_preserve_endpoint_key_bytes() {
    let f = Fixture::new(vec![
        Reply::status(404),
        ok(""),
        Reply {
            status: 200,
            body: String::new(),
            headers: vec![
                ("content-type".into(), "text/csv; charset=utf-8".into()),
                ("content-length".into(), "8".into()),
            ],
        },
        ok("original"),
    ])
    .await;
    let c = client(&f);
    let key = "inventory/a +%/ไทย.csv";
    assert!(c.metadata(key).await.unwrap().is_none());
    c.put(key, "text/csv; charset=utf-8", b"original".to_vec())
        .await
        .unwrap();
    let object = c.metadata(key).await.unwrap().unwrap();
    assert_eq!(object.storage_key, key);
    assert_eq!(object.size, 8);
    assert_eq!(object.content_type, "text/csv; charset=utf-8");
    assert_eq!(c.download(key, 8).await.unwrap(), b"original");
    assert_eq!(c.bucket(), "reports");
    assert_eq!(f.raw_bodies()[1], b"original");
    assert_eq!(
        f.methods(),
        [
            reqwest::Method::HEAD,
            reqwest::Method::PUT,
            reqwest::Method::HEAD,
            reqwest::Method::GET
        ]
    );
    for (uri, headers, _) in f.requests() {
        assert_eq!(
            uri.split('?').next().unwrap(),
            "/graphql.json/reports/inventory/a%20%2B%25/%E0%B9%84%E0%B8%97%E0%B8%A2.csv"
        );
        assert!(
            uri.split_once('?')
                .is_none_or(|(_, query)| matches!(query, "x-id=PutObject" | "x-id=GetObject"))
        );
        let auth = headers[reqwest::header::AUTHORIZATION].to_str().unwrap();
        assert!(auth.contains("Credential=synthetic-access-id/"));
        assert!(auth.contains("/ap-southeast-1/s3/aws4_request"));
        assert!(!auth.contains("synthetic-secret-key"));
    }
}
#[tokio::test]
async fn presign_is_local_encodes_attachment_and_bounds_expiry() {
    let c = S3Client::new(&config("https://example.test/storage/v1/s3")).unwrap();
    let signed = c
        .presign_download("a +%/ไทย.csv", "inventory_2026-10-23.csv", 300)
        .await
        .unwrap();
    let url = reqwest::Url::parse(&signed.url).unwrap();
    assert_eq!(url.host_str(), Some("example.test"));
    assert_eq!(
        url.path(),
        "/storage/v1/s3/reports/a%20%2B%25/%E0%B9%84%E0%B8%97%E0%B8%A2.csv"
    );
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["X-Amz-Expires"], "300");
    assert_eq!(
        query["response-content-disposition"],
        "attachment; filename=\"inventory_2026-10-23.csv\""
    );
    assert!(query["X-Amz-Credential"].starts_with("synthetic-access-id/"));
    assert!(!signed.url.contains("synthetic-secret-key"));
    let signed_at = chrono::NaiveDateTime::parse_from_str(&query["X-Amz-Date"], "%Y%m%dT%H%M%SZ")
        .unwrap()
        .and_utc();
    assert_eq!(signed.expires_at.timestamp(), signed_at.timestamp() + 300);
    let mut maximum = config("https://example.test/storage/v1/s3");
    maximum.signed_url_ttl_secs = 900;
    let maximum = S3Client::new(&maximum)
        .unwrap()
        .presign_download("key.csv", "file.csv", 900)
        .await
        .unwrap();
    assert!(maximum.url.contains("X-Amz-Expires=900"));
    for ttl in [0, 301, u32::MAX] {
        assert!(c.presign_download("k", "a.csv", ttl).await.is_err());
    }
    for filename in ["a.txt", "../a.csv", "a\r\n.csv", "a\".csv", ""] {
        assert!(c.presign_download("k", filename, 10).await.is_err());
    }
}
#[tokio::test]
async fn signing_time_and_expiry_stay_exact_across_a_second_boundary() {
    let mut config = config("https://example.test/storage/v1/s3");
    config.force_path_style = false;
    let client = S3Client::new(&config).unwrap();
    for nanos in [0, 999_999_999] {
        let start = std::time::UNIX_EPOCH + Duration::new(1_792_713_599, nanos);
        let signed = client
            .presign_download_at("with +%/ไทย.csv", "report.csv", 300, start)
            .await
            .unwrap();
        let url = reqwest::Url::parse(&signed.url).unwrap();
        assert_eq!(url.host_str(), Some("reports.example.test"));
        assert_eq!(
            url.path(),
            "/storage/v1/s3/with%20%2B%25/%E0%B9%84%E0%B8%97%E0%B8%A2.csv"
        );
        let date = url
            .query_pairs()
            .find(|(key, _)| key == "X-Amz-Date")
            .unwrap()
            .1
            .into_owned();
        let signed_at = chrono::NaiveDateTime::parse_from_str(&date, "%Y%m%dT%H%M%SZ")
            .unwrap()
            .and_utc();
        assert_eq!(signed_at.timestamp(), 1_792_713_599);
        assert_eq!(signed.expires_at.timestamp(), signed_at.timestamp() + 300);
        assert_eq!(
            signed
                .expires_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "2026-10-23T00:04:59Z"
        );
    }
    config.endpoint = "http://127.0.0.1:65535/storage/v1/s3".into();
    let signed = S3Client::new(&config)
        .unwrap()
        .presign_download("key.csv", "report.csv", 1)
        .await
        .unwrap();
    assert_eq!(
        reqwest::Url::parse(&signed.url).unwrap().path(),
        "/storage/v1/s3/reports/key.csv"
    );
    assert_eq!(signed.expires_at.timestamp_subsec_nanos(), 0);
}

#[test]
fn rejects_missing_invalid_credentials_endpoint_and_bounds_without_panicking() {
    for access in [None, Some(""), Some(" "), Some("a\n")] {
        let mut cfg = config("https://example.test");
        cfg.access_key_id = access.map(str::to_owned);
        assert!(S3Client::new(&cfg).is_err());
    }
    for secret in [None, Some(""), Some(" ")] {
        let mut cfg = config("https://example.test");
        cfg.secret_access_key = secret.map(str::to_owned);
        assert!(S3Client::new(&cfg).is_err());
    }
    for endpoint in [
        "garbage",
        "https://user:pass@example.test",
        "https://example.test/?token=secret",
        "https://example.test/#secret",
        "http://public.example.test",
    ] {
        assert!(S3Client::new(&config(endpoint)).is_err());
    }
    for (timeout, ttl) in [(0, 300), (301, 300), (1, 0), (1, 901)] {
        let mut cfg = config("https://example.test");
        cfg.timeout_secs = timeout;
        cfg.signed_url_ttl_secs = ttl;
        assert!(S3Client::new(&cfg).is_err());
    }
    for bucket in ["", "a", "Reports", "bad/bucket", "bad..bucket"] {
        let mut cfg = config("https://example.test");
        cfg.bucket = bucket.into();
        assert!(S3Client::new(&cfg).is_err());
    }
}
#[tokio::test]
async fn service_errors_and_redirects_never_retry_or_leak_authentication() {
    for status in [301, 307, 400, 401, 403, 429, 500, 503] {
        let f = Fixture::new(vec![Reply {
            status,
            body: "<Error><Code>AccessDenied</Code><Message>synthetic-secret-key</Message></Error>"
                .into(),
            headers: vec![
                (
                    "location".into(),
                    "https://foreign.example.test/stolen".into(),
                ),
                ("x-amz-bucket-region".into(), "us-east-1".into()),
            ],
        }])
        .await;
        let c = client(&f);
        let error = c.put("a.csv", "text/csv", vec![1]).await.unwrap_err();
        assert_eq!(f.requests().len(), 1);
        assert!(!format!("{error:?} {error}").contains("synthetic-secret-key"));
        assert!(std::error::Error::source(&error).is_some());
    }
}
#[tokio::test]
async fn metadata_requires_usable_content_type_and_size() {
    let f = Fixture::new(vec![Reply::status(200)]).await;
    assert!(matches!(
        client(&f).metadata("a.csv").await.unwrap_err().kind(),
        ClientErrorKind::Protocol(_)
    ));
}
#[tokio::test]
async fn bounded_download_checks_declared_size_and_streamed_size() {
    let f = Fixture::new(vec![ok("12345")]).await;
    assert!(matches!(
        client(&f).download("a.csv", 4).await.unwrap_err().kind(),
        ClientErrorKind::Protocol("S3 response exceeds expected size")
    ));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n123\r\n3\r\n456\r\n0\r\n\r\n").await.unwrap();
    });
    assert!(matches!(
        S3Client::new(&config(&endpoint))
            .unwrap()
            .download("a.csv", 4)
            .await
            .unwrap_err()
            .kind(),
        ClientErrorKind::Protocol("S3 response exceeds expected size")
    ));
    server.await.unwrap();
}
#[tokio::test]
async fn deadlines_cover_response_headers_and_body_without_replay() {
    for stall_body in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut request = [0; 8192];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            if stall_body {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\na")
                    .await
                    .unwrap();
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let now = std::time::Instant::now();
        let error = S3Client::new(&config(&endpoint))
            .unwrap()
            .download("a.csv", 5)
            .await
            .unwrap_err();
        assert!(crate::error::client_timeout(&error));
        assert!(now.elapsed() < Duration::from_secs(3));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        server.abort();
    }
}

#[tokio::test]
async fn ambiguous_put_disconnect_is_returned_without_hidden_replay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut raw = Vec::new();
        let mut buf = [0; 4096];
        loop {
            let size = socket.read(&mut buf).await.unwrap();
            if size == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..size]);
            if raw.ends_with(b"original") {
                break;
            }
        }
        drop(socket);
        let second = tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
        assert!(second.is_err(), "SDK must not replay an ambiguous upload");
        raw
    });
    assert!(
        S3Client::new(&config(&endpoint))
            .unwrap()
            .put("a.csv", "text/csv", b"original".to_vec())
            .await
            .is_err()
    );
    assert!(server.await.unwrap().ends_with(b"original"));
}
