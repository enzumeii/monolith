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
async fn raw_transport_preserves_success_and_error_bodies_and_existing_error_messages() {
    use std::io::{Read, Write};
    for (status, body) in [
        (
            200,
            " {\n \"total\":2,\"success\":1,\"failed\":1,\"errors\":[{\"product_id\":1,\"reason\":\"<failed>\"}]}\n",
        ),
        (503, "{\"error\":\"Shopify client unavailable\"}\n"),
        (401, "unauthorized"),
        (502, "<html>upstream failure</html>"),
        (204, ""),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("connection");
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).expect("request") > 0);
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("response");
        });
        let result = send_raw(reqwest::Client::new().post(url)).await;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                assert_eq!(error.status, status);
                assert_eq!(
                    error.to_string(),
                    if status == 503 {
                        "Shopify client unavailable"
                    } else {
                        "Request failed"
                    }
                );
                error.response.expect("original API response")
            }
        };
        assert_eq!(response.status, status);
        assert_eq!(response.body, body);
        server.join().expect("server");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unreachable_api_has_no_http_response() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let error = send_raw(reqwest::Client::new().post(url))
        .await
        .expect_err("unreachable API");
    assert_eq!(error.status, 503);
    assert!(error.response.is_none());
}
