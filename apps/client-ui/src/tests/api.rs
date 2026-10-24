use super::*;
use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;

#[test]
fn requests_require_loaded_settings_and_use_the_public_base_without_admin_credentials() {
    let dom = VirtualDom::new(|| rsx! {});
    let error = dom.in_scope(ScopeId::ROOT, || {
        request(Method::GET, "/conventions").expect_err("configuration is required")
    });
    assert_eq!(error.status, 503);
    assert_eq!(error.message, "Configuration is not loaded");
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig {
            base_url: "https://api.example.test/custom/v1".into(),
        },
    });
    let request = dom.in_scope(ScopeId::ROOT, || {
        request(Method::POST, "/conventions/summer-fair/quote")
            .expect("configured request")
            .build()
            .expect("valid request")
    });
    assert_eq!(
        request.url().as_str(),
        "https://api.example.test/custom/v1/conventions/summer-fair/quote"
    );
    assert_eq!(request.method(), Method::POST);
    assert!(!request.headers().contains_key("authorization"));
    assert!(!request.headers().contains_key("cookie"));
    assert!(!request.headers().contains_key("x-session-mode"));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn decoding_preserves_api_errors_and_reports_safe_transport_and_protocol_errors() {
    for (status, body, expected_status, expected_message) in [
        (
            404,
            r#"{"error":"This convention is not available for preorders."}"#,
            404,
            "This convention is not available for preorders.",
        ),
        (
            503,
            "<html>provider failure: private token</html>",
            503,
            "Unable to complete the request. Please retry.",
        ),
        (
            200,
            "not JSON: private token",
            502,
            "The shop returned an invalid response. Please retry.",
        ),
        (
            200,
            r#"{"wrong":"shape"}"#,
            502,
            "The shop returned an invalid response. Please retry.",
        ),
    ] {
        let (result, requests) = responses(&[(status, body)], async {
            decode::<domain::dto::ConventionQuote>(
                request(Method::GET, "/conventions").expect("request"),
            )
            .await
        })
        .await;
        let error = result.expect_err("HTTP or malformed response must fail");
        assert_eq!(error.status, expected_status);
        assert_eq!(error.message, expected_message);
        assert_eq!(requests.len(), 1);
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let error = decode::<domain::dto::ConventionQuote>(reqwest::Client::new().get(url))
        .await
        .expect_err("unreachable API");
    assert_eq!(error.status, 503);
    assert_eq!(
        error.message,
        "The shop is temporarily unavailable. Please retry."
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn convention(id: &str) -> domain::dto::PublicConvention {
    domain::dto::PublicConvention {
        id: id.into(),
        slug: format!("fair-{id}"),
        name: format!("Fair {id}"),
        logo_preview_url: None,
        logo_preview_url_expires_at: None,
        country_code: "TH".into(),
        event_starts_on: "2099-01-02".into(),
        event_ends_on: "2099-01-04".into(),
        description_markdown: "A customer fair".into(),
        currency_code: "USD".into(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn catalogue() -> domain::dto::PublicCatalogueResponse {
    domain::dto::PublicCatalogueResponse {
        convention: convention("a"),
        products: vec![crate::tests::product()],
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn responses<T>(
    responses: &[(u16, &str)],
    operation: impl std::future::Future<Output = T>,
) -> (T, Vec<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let base_url = format!("http://{}/v1", listener.local_addr().expect("address"));
    let responses: Vec<_> = responses
        .iter()
        .map(|(status, body)| (*status, (*body).to_owned()))
        .collect();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        while matches!(
            stopped.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("accept: {error}"),
            };
            stream.set_nonblocking(false).expect("blocking stream");
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
            let (status, body) = responses
                .get(requests.len())
                .map_or((500, "unexpected request"), |(status, body)| {
                    (*status, body.as_str())
                });
            requests.push(String::from_utf8(bytes).expect("UTF8 request"));
            write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("response");
        }
        requests
    });
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig { base_url },
    });
    let mut operation = std::pin::pin!(operation);
    let result =
        futures::future::poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx)))
            .await;
    stop.send(()).expect("stop server");
    (result, server.join().expect("server"))
}
