use super::*;

const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";

fn mount(compact: bool) -> VirtualDom {
    let mut dom = VirtualDom::new_with_props(
        ReportFiles,
        ReportFilesProps {
            report_id: ID.into(),
            compact,
            files: vec![ReportFileDTO {
                filename: "inventory.csv".into(),
                content_type: "text/csv".into(),
            }],
        },
    );
    dom.rebuild_in_place();
    dom
}

#[test]
fn saved_files_render_accessible_native_download_buttons_in_both_views() {
    for compact in [false, true] {
        let dom = mount(compact);
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("<button"), "{html}");
        assert!(html.contains("type=\"button\""), "{html}");
        assert!(
            html.contains("aria-label=\"Download inventory.csv (text/csv)\""),
            "{html}"
        );
        assert!(
            html.contains("title=\"inventory.csv · text/csv\""),
            "{html}"
        );
        assert!(!html.contains("href="), "{html}");
        if compact {
            assert!(html.contains("report-file-download"), "{html}");
            assert!(html.contains("aria-hidden=\"true\""), "{html}");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod interaction {
    use super::*;
    use dioxus::dioxus_core::{ElementId, Mutation};
    use futures::FutureExt;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        rc::Rc,
        time::Duration,
    };

    fn pump(dom: &mut VirtualDom) {
        for _ in 0..10 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
        }
    }

    fn click(dom: &VirtualDom, id: ElementId) {
        let data = dioxus::html::PlatformEventData::new(Box::new(String::new()));
        dom.runtime().handle_event(
            "click",
            Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
            id,
        );
    }

    async fn request(dom: &mut VirtualDom, listener: &TcpListener) -> TcpStream {
        for _ in 0..300 {
            pump(dom);
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_nonblocking(true).expect("nonblocking stream");
                let mut bytes = Vec::new();
                for _ in 0..300 {
                    let mut chunk = [0; 4096];
                    match stream.read(&mut chunk) {
                        Ok(count) if count > 0 => bytes.extend_from_slice(&chunk[..count]),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                        other => panic!("request read: {other:?}"),
                    }
                    if bytes.windows(4).any(|value| value == b"\r\n\r\n") {
                        stream.set_nonblocking(false).expect("blocking response");
                        return stream;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    pump(dom);
                }
                panic!("incomplete request");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("download request did not arrive");
    }

    async fn visible(dom: &mut VirtualDom, expected: &str) -> String {
        for _ in 0..300 {
            pump(dom);
            let html = dioxus_ssr::render(dom);
            if html.contains(expected) {
                return html;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("missing {expected}: {}", dioxus_ssr::render(dom));
    }

    #[tokio::test]
    async fn in_flight_download_disables_button_and_failure_offers_working_retry() {
        dioxus::html::set_event_converter(Box::new(crate::test_events::FormEventConverter));
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let mut dom = VirtualDom::new_with_props(
            ReportFiles,
            ReportFilesProps {
                report_id: ID.into(),
                compact: true,
                files: vec![ReportFileDTO {
                    filename: "inventory.csv".into(),
                    content_type: "text/csv".into(),
                }],
            },
        )
        .with_root_context(crate::config::Settings {
            conventions: crate::config::ConventionsConfig {
                catalogue_location_id: "65761018027".into(),
            },
            api: crate::config::ApiConfig {
                base_url: format!("http://{}/v1", listener.local_addr().expect("address")),
            },
        });
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "synthetic".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        let initial = dom.rebuild_to_vec();
        let button = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "click" => Some(*id),
                _ => None,
            })
            .expect("download button");
        click(&dom, button);
        let mut pending = request(&mut dom, &listener).await;
        let html = visible(&mut dom, "Preparing inventory.csv").await;
        assert!(html.contains("disabled"), "{html}");
        assert!(html.contains("aria-busy=true"), "{html}");
        click(&dom, button);
        pump(&mut dom);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        let body = r#"{"error":"Report storage unavailable"}"#;
        write!(
            pending,
            "HTTP/1.1 503 Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("response");
        drop(pending);
        let html = visible(&mut dom, "Retry download inventory.csv").await;
        assert!(html.contains("role=\"alert\""), "{html}");
        assert!(html.contains("Report storage unavailable"), "{html}");
        assert!(!html.contains(" disabled"), "{html}");
        click(&dom, button);
        let _retried = request(&mut dom, &listener).await;
        let html = visible(&mut dom, "Preparing inventory.csv").await;
        assert!(html.contains("disabled"), "{html}");
        assert!(!html.contains("role=\"alert\""), "{html}");
    }
}
