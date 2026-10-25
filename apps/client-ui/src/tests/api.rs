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
        ..crate::config::tests::settings()
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
            "Unable to complete the request.",
        ),
        (
            200,
            "not JSON: private token",
            502,
            "The shop returned an invalid response.",
        ),
        (
            200,
            r#"{"wrong":"shape"}"#,
            502,
            "The shop returned an invalid response.",
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
        assert_eq!(error.is_transient(), status == 503);
        assert_eq!(requests.len(), 1);
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let error = decode::<domain::dto::ConventionQuote>(reqwest::Client::new().get(url))
        .await
        .expect_err("unreachable API");
    assert_eq!(error.status, 503);
    assert_eq!(error.message, "The shop is temporarily unavailable.");
    assert!(error.is_transient());
    let error = decode::<domain::dto::ConventionQuote>(reqwest::Client::new().get("not a URL"))
        .await
        .expect_err("invalid request URL");
    assert!(!error.is_transient());
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
    let responses: Vec<_> = responses
        .iter()
        .map(|(status, body)| (*status, *body, body.len()))
        .collect();
    responses_with_lengths(&responses, operation).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn responses_with_lengths<T>(
    responses: &[(u16, &str, usize)],
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
        .map(|(status, body, length)| (*status, (*body).to_owned(), *length))
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
            let (status, body, length) = responses
                .get(requests.len())
                .map_or((500, "unexpected request", 18), |(status, body, length)| {
                    (*status, body.as_str(), *length)
                });
            requests.push(String::from_utf8(bytes).expect("UTF8 request"));
            write!(
                stream,
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}"
            )
            .expect("response");
        }
        requests
    });
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig { base_url },
        ..crate::config::tests::settings()
    });
    let mut operation = std::pin::pin!(operation);
    let result =
        futures::future::poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx)))
            .await;
    stop.send(()).expect("stop server");
    (result, server.join().expect("server"))
}

#[test]
fn only_actual_temporary_http_failures_are_transient() {
    for status in [408, 429, 500, 502, 503, 504, 599] {
        let status = StatusCode::from_u16(status).expect("status");
        assert!(ApiError::http(status, "remote failure").is_transient());
        assert!(!ApiError::new(status, "local or protocol failure").is_transient());
    }
    for status in [400, 401, 403, 404, 409, 422] {
        let status = StatusCode::from_u16(status).expect("status");
        assert!(!ApiError::http(status, "terminal HTTP failure").is_transient());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn automatic_retry_preserves_requests_and_caps_waits_before_success() {
    use crate::pages::tests::TestDocument;
    use dioxus::document::Document;
    use futures::FutureExt;
    use std::{cell::Cell, rc::Rc};
    let document = TestDocument::new(vec![serde_json::json!(true); 6]);
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(document.clone() as Rc<dyn Document>);
    let calls = Cell::new(0);
    let result = dom.in_scope(ScopeId::ROOT, || {
        retry(|| async {
            calls.set(calls.get() + 1);
            if calls.get() < 7 {
                Err(ApiError::http(StatusCode::TOO_MANY_REQUESTS, "Busy"))
            } else {
                Ok("loaded")
            }
        })
        .now_or_never()
        .expect("controlled waits finish")
    });
    assert_eq!(result, Ok("loaded"));
    assert_eq!(calls.get(), 7);
    let scripts = document.scripts.borrow();
    for (script, millis) in scripts
        .iter()
        .zip([5_000, 10_000, 20_000, 30_000, 30_000, 30_000])
    {
        assert!(script.contains(&format!("setTimeout(resolve, {millis})")));
    }
    assert_eq!(scripts.len(), 6);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn automatic_retry_stops_on_terminal_errors_browser_failure_and_cancellation() {
    use crate::pages::tests::TestDocument;
    use dioxus::document::Document;
    use futures::FutureExt;
    use std::{cell::Cell, rc::Rc};
    for terminal in [true, false] {
        let document = TestDocument::new(vec![serde_json::json!(false)]);
        let dom =
            VirtualDom::new(|| rsx! {}).with_root_context(document.clone() as Rc<dyn Document>);
        let calls = Cell::new(0);
        let result: ApiResult<()> = dom.in_scope(ScopeId::ROOT, || {
            retry(|| async {
                calls.set(calls.get() + 1);
                Err(if terminal {
                    ApiError::new(StatusCode::BAD_GATEWAY, "Invalid response")
                } else {
                    ApiError::http(StatusCode::SERVICE_UNAVAILABLE, "Busy")
                })
            })
            .now_or_never()
            .expect("controlled wait finishes")
        });
        assert!(!result.expect_err("recovery stops").is_transient());
        assert_eq!(calls.get(), 1);
        assert_eq!(document.scripts.borrow().len(), usize::from(!terminal));
    }
    let document = TestDocument::new(vec![serde_json::json!({"__eval_pending":true})]);
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(document.clone() as Rc<dyn Document>);
    let calls = Cell::new(0);
    let mut operation = Box::pin(retry(|| async {
        calls.set(calls.get() + 1);
        Err::<(), _>(ApiError::http(StatusCode::SERVICE_UNAVAILABLE, "Busy"))
    }));
    assert!(
        dom.in_scope(ScopeId::ROOT, || operation.as_mut().now_or_never())
            .is_none()
    );
    drop(operation);
    assert_eq!(calls.get(), 1);
    assert_eq!(document.scripts.borrow().len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn interrupted_success_body_recovers_but_complete_malformed_json_stops() {
    use crate::pages::tests::TestDocument;
    use dioxus::document::Document;
    use std::rc::Rc;
    for interrupted in [true, false] {
        let body = "{\"ready\":";
        let complete = "{\"ready\":true}";
        let ((result, waits), requests) = responses_with_lengths(
            &[
                (200, body, body.len() + usize::from(interrupted)),
                (200, complete, complete.len()),
            ],
            async {
                let document = TestDocument::new(vec![serde_json::json!(true)]);
                let settings = consume_context::<Settings>();
                let dom = VirtualDom::new(|| rsx! {})
                    .with_root_context(settings)
                    .with_root_context(document.clone() as Rc<dyn Document>);
                let mut operation = std::pin::pin!(retry(|| async {
                    decode::<serde_json::Value>(
                        request(Method::GET, "/conventions").expect("request"),
                    )
                    .await
                }));
                let result = futures::future::poll_fn(|cx| {
                    dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx))
                })
                .await;
                let waits = document.scripts.borrow().len();
                (result, waits)
            },
        )
        .await;
        if interrupted {
            assert_eq!(
                result.expect("body transfer recovered"),
                serde_json::json!({"ready":true})
            );
            assert_eq!(requests.len(), 2);
            assert_eq!(waits, 1);
        } else {
            assert!(
                !result
                    .expect_err("complete malformed JSON is terminal")
                    .is_transient()
            );
            assert_eq!(requests.len(), 1);
            assert_eq!(waits, 0);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct LoadingProbeRequest {
    url: String,
    unit: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn LoadingProbe() -> Element {
    let request = use_context::<LoadingProbeRequest>();
    let response = use_resource(move || {
        let request = request.clone();
        async move {
            let _ = request.unit;
            decode::<serde_json::Value>(reqwest::Client::new().get(request.url))
                .await
                .map(|_| ())
                .map_err(|error| error.status)
        }
    });
    rsx! { p { if response.read().is_some() { "Operation finished" } else { "Operation pending" } } }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn loading_covers_delayed_headers_and_body_then_releases_for_every_outcome() {
    for (status, body, interrupted, cancelled, unit) in [
        (200, r#"{"ready":true}"#, false, false, false),
        (
            400,
            r#"{"error":"Controlled rejection"}"#,
            false,
            false,
            false,
        ),
        (200, "malformed JSON", false, false, false),
        (200, "interrupted", true, false, false),
        (200, r#"{"ready":true}"#, false, true, false),
    ] {
        loading_case(status, body, interrupted, cancelled, unit).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn loading_case(
    status: u16,
    body: &'static str,
    interrupted: bool,
    cancelled: bool,
    unit: bool,
) {
    use futures::FutureExt;
    use std::time::Duration;
    fn app() -> Element {
        rsx! { ui::LoadingBoundary { LoadingProbe {} } }
    }
    fn pump(dom: &mut VirtualDom) {
        for _ in 0..5 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
        }
    }

    let DelayedLoadingServer {
        url,
        seen_rx,
        headers_tx,
        body_tx,
        server,
    } = delayed_loading_server(status, body, interrupted);
    let mut dom = VirtualDom::new(app).with_root_context(LoadingProbeRequest { url, unit });
    dom.rebuild_in_place();
    let mut seen = false;
    for _ in 0..200 {
        pump(&mut dom);
        if seen_rx.try_recv().is_ok() {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(seen, "mounted operation reached controlled HTTP server");
    pump(&mut dom);
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("page-loading"),
        "loading before headers: {html}"
    );
    assert!(html.contains("inert"), "underlying page is blocked: {html}");
    headers_tx.send(()).expect("headers");
    for _ in 0..10 {
        pump(&mut dom);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if !unit {
        assert!(
            dioxus_ssr::render(&dom).contains("page-loading"),
            "body transfer retains guard"
        );
    }
    if cancelled {
        drop(dom);
        body_tx.send(()).expect("body");
        server.join().expect("server");
        return;
    }
    body_tx.send(()).expect("body");
    let mut finished = false;
    for _ in 0..200 {
        pump(&mut dom);
        if dioxus_ssr::render(&dom).contains("Operation finished") {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let html = dioxus_ssr::render(&dom);
    server.join().expect("server");
    assert!(finished, "operation completed: {html}");
    assert!(
        !html.contains("page-loading"),
        "success/error releases the overlay: {html}"
    );
    assert!(
        !html.contains("inert="),
        "page is interactive again: {html}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
struct DelayedLoadingServer {
    url: String,
    seen_rx: std::sync::mpsc::Receiver<()>,
    headers_tx: std::sync::mpsc::Sender<()>,
    body_tx: std::sync::mpsc::Sender<()>,
    server: std::thread::JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
fn delayed_loading_server(
    status: u16,
    body: &'static str,
    interrupted: bool,
) -> DelayedLoadingServer {
    use std::{
        io::{Read, Write},
        sync::mpsc,
        time::Duration,
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let (seen_tx, seen_rx) = mpsc::channel();
    let (headers_tx, headers_rx) = mpsc::channel();
    let (body_tx, body_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).expect("request") > 0);
        seen_tx.send(()).expect("request observed");
        headers_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("release headers");
        write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len() + usize::from(interrupted)
        )
        .expect("headers");
        stream.flush().expect("flush");
        body_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("release body");
        let _ = stream.write_all(body.as_bytes());
    });
    DelayedLoadingServer {
        url,
        seen_rx,
        headers_tx,
        body_tx,
        server,
    }
}
