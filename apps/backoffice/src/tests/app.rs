use super::*;
use dioxus::document::{Document, Eval, EvalError, Evaluator};
use futures::FutureExt;
use std::{
    io::{Read, Write},
    rc::Rc,
    sync::mpsc,
    task::{Context, Poll},
    time::Duration,
};

#[allow(dead_code)]
#[path = "events.rs"]
mod events;

struct PageDocument {
    owner: dioxus::dioxus_core::internal::generational_box::Owner,
    url: String,
}
struct PageMessage(Option<String>);
impl Evaluator for PageMessage {
    fn send(&self, _: serde_json::Value) -> Result<(), EvalError> {
        Ok(())
    }
    fn poll_recv(&mut self, _: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        Poll::Ready(
            self.0
                .take()
                .map(serde_json::Value::String)
                .ok_or(EvalError::Unsupported),
        )
    }
    fn poll_join(&mut self, cx: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        self.poll_recv(cx)
    }
}
impl Document for PageDocument {
    fn eval(&self, _: String) -> Eval {
        Eval::new(
            self.owner
                .insert(Box::new(PageMessage(Some(self.url.clone())))),
        )
    }
}
fn retry_server() -> (
    String,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    let url = format!("http://{}/", listener.local_addr().expect("address"));
    let (seen_tx, seen_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().expect("connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).expect("request") > 0);
            if attempt == 1 {
                seen_tx.send(()).expect("retry observed");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("release retry");
            }
            write!(
                stream,
                "HTTP/1.1 403 Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .expect("response");
        }
    });
    (url, seen_rx, release_tx, server)
}
#[tokio::test]
async fn configuration_retry_blocks_while_previous_error_remains_rendered() {
    dioxus::html::set_event_converter(Box::new(events::FormEventConverter));
    let (url, seen, release, server) = retry_server();
    let document = Rc::new(PageDocument {
        owner: dioxus::dioxus_core::internal::generational_box::Owner::default(),
        url,
    });
    let mut dom = VirtualDom::new(App).with_root_context(document as Rc<dyn Document>);
    let mut changes = dom.rebuild_to_vec();
    for _ in 0..200 {
        changes.edits.extend(events::render_changes(&mut dom).edits);
        if dioxus_ssr::render(&dom).contains("Retry") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("Unable to load application.toml"), "{html}");
    let retry = events::listeners(&changes, "click")[0];
    events::form_event(&dom, retry, "click", "");
    let mut observed = false;
    for _ in 0..200 {
        let _ = dom.wait_for_work().now_or_never();
        events::render_changes(&mut dom);
        if seen.try_recv().is_ok() {
            observed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    events::render_changes(&mut dom);
    let html = dioxus_ssr::render(&dom);
    release.send(()).expect("release");
    server.join().expect("server");
    assert!(observed, "configuration Retry reached HTTP server");
    assert!(
        html.contains("Unable to load application.toml"),
        "retained error remains mounted"
    );
    assert!(
        html.contains("page-loading"),
        "active retry must block the retained error page: {html}"
    );
    assert!(
        html.contains("inert="),
        "active retry must block underlying Retry button"
    );
    for _ in 0..200 {
        events::render_changes(&mut dom);
        if !dioxus_ssr::render(&dom).contains("page-loading") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("failed retry did not release overlay");
}
