use super::*;
use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;

#[test]
fn requests_use_the_loaded_public_api_base() {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        conventions: crate::config::ConventionsConfig {
            catalogue_location_id: "65761018027".into(),
        },
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
    let (result, mut requests) = metadata_responses(&[(status, body)], operation).await;
    assert_eq!(requests.len(), 1, "expected one metadata request");
    (result, requests.remove(0))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn metadata_responses<T>(
    responses: &[(u16, &str)],
    operation: impl std::future::Future<Output = T>,
) -> (T, Vec<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let base_url = format!("http://{}/v1/", listener.local_addr().expect("address"));
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
        conventions: crate::config::ConventionsConfig {
            catalogue_location_id: "65761018027".into(),
        },
        api: ApiConfig { base_url },
    });
    let mut operation = std::pin::pin!(operation);
    let result =
        futures::future::poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx)))
            .await;
    stop.send(()).expect("stop server");
    (result, server.join().expect("server"))
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct LoadingProbeRequest {
    url: String,
    mode: &'static str,
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn LoadingProbe() -> Element {
    let request = use_context::<LoadingProbeRequest>();
    let response = use_resource(move || {
        let request = request.clone();
        async move {
            if request.mode == "restore" {
                return crate::session::restore()
                    .await
                    .map_err(|error| error.status);
            }
            if request.mode == "login" {
                return crate::session::sign_in("test-admin".into(), "test-password".into())
                    .await
                    .map_err(|error| error.status);
            }
            if request.mode == "unit" {
                return send(reqwest::Client::new().get(request.url))
                    .await
                    .map(|_| ())
                    .map_err(|error| error.status);
            }
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
    for (status, body, interrupted, cancelled, mode) in [
        (200, r#"{"ready":true}"#, false, false, "decode"),
        (
            400,
            r#"{"error":"Controlled rejection"}"#,
            false,
            false,
            "decode",
        ),
        (200, "malformed JSON", false, false, "decode"),
        (200, "interrupted", true, false, "decode"),
        (200, r#"{"ready":true}"#, false, true, "decode"),
        (204, "", false, false, "unit"),
        (
            200,
            r#"{"admin_id":7,"username":"session-admin","role":"ADMIN","expires_at":4102444800}"#,
            false,
            false,
            "restore",
        ),
        (
            200,
            r#"{"admin_id":7,"username":"session-admin","role":"ADMIN","expires_at":4102444800}"#,
            false,
            false,
            "login",
        ),
        (
            400,
            r#"{"error":"Controlled rejection"}"#,
            false,
            false,
            "login",
        ),
    ] {
        loading_case(status, body, interrupted, cancelled, mode).await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn loading_case(
    status: u16,
    body: &'static str,
    interrupted: bool,
    cancelled: bool,
    mode: &'static str,
) {
    use futures::FutureExt;
    use std::time::Duration;
    fn app() -> Element {
        rsx! { ui::LoadingBoundary {
            crate::components::Popup { open: true, title: "Existing editor", on_close: move |()| {},
                input { value: "retained editor draft" }
            }
            LoadingProbe {}
        } }
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
    let mut dom = VirtualDom::new(app)
        .with_root_context(Settings {
            api: ApiConfig {
                base_url: url.clone(),
            },
            conventions: crate::config::ConventionsConfig {
                catalogue_location_id: "1".into(),
            },
        })
        .with_root_context(LoadingProbeRequest { url, mode });
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
    if mode != "unit" {
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
    assert!(html.contains("Existing editor"));
    assert!(html.contains("retained editor draft"));
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
