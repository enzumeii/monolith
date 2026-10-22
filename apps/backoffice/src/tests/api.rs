use super::*;
use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;

#[test]
fn requests_use_the_loaded_public_api_base() {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig {
            base_url: "https://api.example.test/custom/v1/".into(),
        },
    });
    let request = dom.in_scope(ScopeId::ROOT, || {
        request(Method::POST, "/auth/refresh")
            .expect("configured request")
            .build()
            .expect("valid request")
    });
    assert_eq!(
        request.url().as_str(),
        "https://api.example.test/custom/v1/auth/refresh"
    );
    assert_eq!(request.headers()["x-session-mode"], "cookie");
}

#[test]
fn requests_fail_before_configuration_is_loaded() {
    let dom = VirtualDom::new(|| rsx! {});
    let error = dom.in_scope(ScopeId::ROOT, || {
        request(Method::GET, "/admins").expect_err("configuration is required")
    });
    assert_eq!(error.status, 503);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn trigger_responses_decode_counts_details_and_safe_error_messages() {
    use std::io::{Read, Write};
    for (status, body, expected_counts) in [
        (
            200,
            " {\n \"total\":2,\"success\":1,\"failed\":1,\"errors\":[{\"product_id\":1,\"reason\":\"<failed>\"}]}\n",
            Some((2, 1, 1)),
        ),
        (
            200,
            "{\"total\":0,\"success\":0,\"failed\":0}",
            Some((0, 0, 0)),
        ),
        (503, "{\"error\":\"Shopify client unavailable\"}\n", None),
        (401, "unauthorized", None),
        (502, "<html>upstream failure</html>", None),
        (200, "not JSON", None),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("connection");
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).expect("request") > 0);
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("response");
        });
        let result =
            decode::<domain::dto::InventoryEnableResponse>(reqwest::Client::new().post(url)).await;
        if let Some(counts) = expected_counts {
            let response = result.expect("successful batch response");
            assert_eq!((response.total, response.success, response.failed), counts);
            if response.failed > 0 {
                assert_eq!(response.errors[0].product_id, 1);
                assert_eq!(response.errors[0].reason, "<failed>");
            } else {
                assert!(response.errors.is_empty());
            }
        } else {
            let error = result.expect_err("invalid response or API error");
            assert_eq!(error.status, if status == 200 { 502 } else { status });
            assert_eq!(
                error.to_string(),
                match status {
                    503 => "Shopify client unavailable",
                    200 => "Invalid API response",
                    _ => "Request failed",
                }
            );
        }
        server.join().expect("server");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unreachable_api_reports_a_safe_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let error = send(reqwest::Client::new().post(url))
        .await
        .expect_err("unreachable API");
    assert_eq!(error.status, 503);
    assert_eq!(error.to_string(), "API unavailable");
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn metadata_response<T>(
    status: u16,
    body: &str,
    operation: impl std::future::Future<Output = T>,
) -> (T, String) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let base_url = format!("http://{}/v1/", listener.local_addr().expect("address"));
    let body = body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("timeout");
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).expect("request");
            assert!(count > 0, "request was incomplete");
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().expect("length"))
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("response");
        String::from_utf8(bytes).expect("UTF8 request")
    });
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig { base_url },
    });
    let mut operation = std::pin::pin!(operation);
    let result =
        futures::future::poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx)))
            .await;
    (result, server.join().expect("server"))
}
