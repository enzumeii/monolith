#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::{api::tests::metadata_responses, session};
use dioxus::history::{MemoryHistory, provide_history_context};
use futures::FutureExt;
use std::rc::Rc;

const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";
const RECORD: &str = r##"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","slug":"event","name":"Loaded event","logo_preview_url":null,"country_code":"TH","event_starts_on":"2026-11-01","event_ends_on":"2026-11-02","description_markdown":"# Event","preorder_enabled":false,"currency_code":"THB","payment_methods":["stripe"],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"##;

fn app() -> Element {
    let path = use_context::<String>();
    use_hook(move || provide_history_context(Rc::new(MemoryHistory::with_initial_path(path))));
    rsx! { Router::<crate::route::Route> {} }
}

async fn mounted_response(path: &str, responses: &[(u16, &str)], expected: &str) -> String {
    let (html, requests) = metadata_responses(responses, async {
        let settings = consume_context::<crate::config::Settings>();
        let mut dom = VirtualDom::new(app)
            .with_root_context(path.to_owned())
            .with_root_context(settings);
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        dom.rebuild_in_place();
        let initial = dioxus_ssr::render(&dom);
        assert!(
            initial.contains("Loading convention"),
            "initial render: {initial}"
        );
        let mut html = initial;
        for _ in 0..200 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
            html = dioxus_ssr::render(&dom);
            if html.contains(expected) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        html
    })
    .await;
    assert!(!requests.is_empty(), "actual component must call the API");
    if path == "/conventions" {
        assert_eq!(
            requests.len(),
            1,
            "list logo rendering must not fan out requests"
        );
    }
    assert!(
        html.contains(expected),
        "resource completion must render {expected}: {html}"
    );
    assert!(
        !html.contains("Loading convention"),
        "loading must finish: {html}"
    );
    html
}

#[tokio::test]
async fn mounted_convention_list_renders_async_success_and_failure_without_other_state_changes() {
    let list = format!(r#"{{"items":[{RECORD}],"page":1,"per_page":25,"has_next":false}}"#);
    let html = mounted_response("/conventions", &[(200, &list)], "Loaded event").await;
    assert!(html.contains("No logo preview"), "{html}");
    assert!(!html.contains("convention-name-action"));
    assert!(html.contains("convention-list-heading"));
    assert!(html.contains(">Edit</a>"));
    assert!(html.contains("Delete Loaded event"));
    assert!(html.contains("Actions"));
    assert!(html.contains("convention-status is-disabled"));
    let logo_list = list.replace("\"logo_preview_url\":null", "\"logo_preview_url\":\"https://s3.test/logo.png?signature=one\",\"logo_preview_url_expires_at\":\"2099-01-01T00:00:00Z\"");
    let html = mounted_response(
        "/conventions",
        &[(200, &logo_list)],
        "Logo for Loaded event",
    )
    .await;
    assert!(html.contains("class=\"convention-thumbnail\""));
    assert!(html.contains("loading=\"lazy\""));
    mounted_response(
        "/conventions",
        &[(502, r#"{"error":"List unavailable"}"#)],
        "List unavailable",
    )
    .await;
}

#[tokio::test]
async fn mounted_convention_detail_renders_async_success_and_failure_without_other_state_changes() {
    let html = mounted_response(
        &format!("/conventions/{ID}"),
        &[
            (200, RECORD),
            (
                200,
                r#"{"countries":["TH"],"currencies":[{"code":"THB","payment_methods":["stripe"]}]}"#,
            ),
            (
                200,
                r#"{"items":[],"page":1,"per_page":100,"has_next":false}"#,
            ),
        ],
        "Save metadata",
    )
    .await;
    assert!(html.contains("Loaded event"));
    assert!(html.contains("convention-editor-heading"));
    assert!(html.contains("convention-logo-file-box"));
    assert!(html.contains("convention-file-discard"));
    assert!(html.contains("Delete Loaded event"));
    assert!(!html.contains(">Back to conventions</a>"));
    mounted_response(
        &format!("/conventions/{ID}"),
        &[(502, r#"{"error":"Convention unavailable"}"#)],
        "Convention unavailable",
    )
    .await;
}

#[tokio::test]
async fn list_deletion_requires_confirmation_preserves_errors_and_refreshes_after_success() {
    use crate::test_events::{
        FormEventConverter, element_id, form_event, listeners, render_changes,
    };
    let list = format!(r#"{{"items":[{RECORD}],"page":1,"per_page":25,"has_next":false}}"#);
    let ((), requests) = metadata_responses(
        &[
            (200, &list),
            (409, r#"{"error":"Delete failed"}"#),
            (204, ""),
            (
                200,
                r#"{"items":[],"page":1,"per_page":25,"has_next":false}"#,
            ),
        ],
        async {
            dioxus::html::set_event_converter(Box::new(FormEventConverter));
            let settings = consume_context::<crate::config::Settings>();
            let mut dom = VirtualDom::new(app)
                .with_root_context("/conventions".to_owned())
                .with_root_context(settings);
            dom.in_scope(ScopeId::ROOT, || {
                *session::SESSION.write() = Some(session::Session {
                    admin_id: 1,
                    username: "fixture".into(),
                    role: domain::AdminRole::Admin,
                    expires_at: u64::MAX,
                });
            });
            let mut edits = dom.rebuild_to_vec();
            for _ in 0..40 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                edits.edits.extend(render_changes(&mut dom).edits);
            }
            let delete = element_id(&edits, &format!("delete-convention-{ID}"));
            let clicks = listeners(&edits, "click");
            let confirm = *clicks.last().expect("confirm delete");
            let cancel = clicks[clicks.len() - 2];
            form_event(&dom, delete, "click", "");
            render_changes(&mut dom);
            form_event(&dom, cancel, "click", "");
            render_changes(&mut dom);
            for expected in ["Delete failed", "No conventions match this filter."] {
                form_event(&dom, delete, "click", "");
                render_changes(&mut dom);
                form_event(&dom, confirm, "click", "");
                for _ in 0..40 {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    render_changes(&mut dom);
                }
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains(expected), "{html}");
                if expected == "Delete failed" {
                    assert!(html.contains("Loaded event"));
                }
            }
        },
    )
    .await;
    assert_eq!(requests.len(), 4, "cancel must not send a deletion request");
    for request in &requests[1..3] {
        assert!(request.starts_with(&format!("DELETE /v1/admin/conventions/{ID} HTTP/1.1")));
    }
    assert!(requests[3].starts_with("GET /v1/admin/conventions?"));
}
