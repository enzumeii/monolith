use super::*;
use crate::test_events::{FormEventConverter, form_event, listeners, render_changes};
use dioxus::dioxus_core::{ElementId, Mutation, Mutations};
use dioxus::history::{MemoryHistory, provide_history_context};
use std::rc::Rc;

const RECORD: &str = r##"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","slug":"event","name":"Loaded event","logo_preview_url":null,"country_code":"TH","event_starts_on":"2026-11-01","event_ends_on":"2026-11-02","description_markdown":"# Event","preorder_enabled":false,"currency_code":"THB","payment_methods":["stripe"],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"##;
const MEMBERSHIPS: &str = r#"{"items":[],"page":1,"per_page":100,"has_next":false}"#;

fn editor() -> Element {
    use_hook(|| {
        provide_history_context(Rc::new(MemoryHistory::with_initial_path(
            "/conventions/event",
        )));
    });
    rsx! { Router::<TestRoute> {} }
}

#[derive(Clone, PartialEq, Routable)]
enum TestRoute {
    #[route("/conventions/event")]
    EditorPage {},
}

#[component]
fn EditorPage() -> Element {
    let record = try_consume_context::<ConventionDTO>().unwrap_or_else(record);
    rsx! { Editor { record, options: ConventionOptionsResponse { countries: vec!["TH".into()], currencies: vec![domain::dto::ConventionCurrencyOption { code: "THB".into(), payment_methods: vec![domain::dto::ConventionPaymentMethod::Stripe] }] } } }
}

fn record() -> ConventionDTO {
    ConventionDTO {
        id: "da41d46b-f1ca-4a76-a774-62c269d28731".into(),
        slug: "event".into(),
        name: "Loaded event".into(),
        logo_preview_url: None,
        logo_preview_url_expires_at: None,
        country_code: "TH".into(),
        event_starts_on: "2026-11-01".into(),
        event_ends_on: "2026-11-02".into(),
        description_markdown: "# Event".into(),
        preorder_enabled: false,
        currency_code: "THB".into(),
        payment_methods: vec![domain::dto::ConventionPaymentMethod::Stripe],
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    }
}

fn event(dom: &VirtualDom, id: ElementId, name: &str, data: Box<dyn std::any::Any>) {
    dom.runtime().handle_event(
        name,
        Event::new(
            Rc::new(dioxus::html::PlatformEventData::new(data)) as Rc<dyn std::any::Any>,
            true,
        ),
        id,
    );
}

fn choose(dom: &VirtualDom, input: ElementId) {
    let file = crate::pages::conventions::logo::tests::LogoFixture("image/png");
    event(
        dom,
        input,
        "change",
        Box::new(vec![dioxus::html::FileData::new(file)]),
    );
}

async fn settled(dom: &mut VirtualDom) -> Mutations {
    let mut changes = render_changes(dom);
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        changes.edits.extend(render_changes(dom).edits);
    }
    changes
}

fn replaced_input(changes: &Mutations, previous: ElementId) -> ElementId {
    assert!(
        changes
            .edits
            .iter()
            .any(|edit| matches!(edit, Mutation::Remove { id } | Mutation::ReplaceWith { id, .. } if *id == previous)),
        "reset must remove the previous native input: {:?}",
        changes.edits
    );
    *listeners(changes, "change")
        .last()
        .expect("replacement file input listener")
}

fn assert_ready(dom: &VirtualDom) {
    let html = dioxus_ssr::render(dom);
    assert!(
        html.contains("Selected: logo.png (4 bytes), not yet saved"),
        "candidate must be restored: {html}"
    );
    let upload_prefix = html
        .split("Upload logo")
        .next()
        .expect("upload button text");
    let upload_attributes = upload_prefix
        .rsplit("<button")
        .next()
        .expect("upload button");
    assert!(
        !upload_attributes.contains("disabled"),
        "upload must be ready: {html}"
    );
    assert!(
        html.contains("Unsaved event"),
        "metadata draft must survive: {html}"
    );
}

#[tokio::test]
async fn discarding_logo_replaces_native_input_and_same_file_is_ready_again() {
    crate::api::tests::metadata_responses(&[(200, MEMBERSHIPS)], async {
        dioxus::html::set_event_converter(Box::new(FormEventConverter));
        let settings = consume_context::<crate::config::Settings>();
        let mut dom = VirtualDom::new(editor).with_root_context(settings);
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        let initial = dom.rebuild_to_vec();
        let input = *listeners(&initial, "change").last().expect("file input");
        let discard = crate::test_events::element_id(&initial, "convention-logo-discard");
        form_event(
            &dom,
            listeners(&initial, "input")[0],
            "input",
            "Unsaved event",
        );
        settled(&mut dom).await;
        choose(&dom, input);
        settled(&mut dom).await;
        assert_ready(&dom);
        event(&dom, discard, "click", Box::new(String::new()));
        let reset = settled(&mut dom).await;
        let input = replaced_input(&reset, input);
        assert!(!dioxus_ssr::render(&dom).contains("Selected: logo.png"));
        choose(&dom, input);
        settled(&mut dom).await;
        assert_ready(&dom);
    })
    .await;
}

#[tokio::test]
async fn failed_upload_keeps_candidate_and_success_resets_only_native_input() {
    let uploaded = RECORD.replace(
        "\"logo_preview_url\":null",
        "\"logo_preview_url\":\"https://s3.test/key.png?signature=one\",\"logo_preview_url_expires_at\":\"2026-11-01T00:05:00Z\"",
    );
    let ((), requests) = crate::api::tests::metadata_responses(
        &[
            (200, MEMBERSHIPS),
            (502, r#"{"error":"Storage unavailable"}"#),
            (201, r#"{"location":"conventions/event/key.png","url":"https://s3.test/key.png?signature=one","expires_at":"2026-11-01T00:05:00Z"}"#),
            (502, r#"{"error":"Attach unavailable"}"#),
            (200, &uploaded),
        ],
        async {
            dioxus::html::set_event_converter(Box::new(FormEventConverter));
            let settings = consume_context::<crate::config::Settings>();
            let mut dom = VirtualDom::new(editor).with_root_context(settings);
            dom.in_scope(ScopeId::ROOT, || {
                *session::SESSION.write() = Some(session::Session {
                    admin_id: 1,
                    username: "fixture".into(),
                    role: domain::AdminRole::Admin,
                    expires_at: u64::MAX,
                });
            });
            let initial = dom.rebuild_to_vec();
            let input = *listeners(&initial, "change").last().expect("file input");
            let upload = crate::test_events::element_id(&initial, "convention-logo-upload");
            form_event(
                &dom,
                listeners(&initial, "input")[0],
                "input",
                "Unsaved event",
            );
            settled(&mut dom).await;
            choose(&dom, input);
            settled(&mut dom).await;
            assert_ready(&dom);
            event(&dom, upload, "click", Box::new(String::new()));
            let failed = settled(&mut dom).await;
            assert!(
                listeners(&failed, "change").is_empty(),
                "failed upload must not remount input"
            );
            assert!(dioxus_ssr::render(&dom).contains("Storage unavailable"));
            assert_ready(&dom);
            event(&dom, upload, "click", Box::new(String::new()));
            let failed_attach = settled(&mut dom).await;
            assert_eq!(listeners(&failed_attach, "change"), [] as [ElementId; 0]);
            assert!(dioxus_ssr::render(&dom).contains("Attach unavailable"));
            assert_ready(&dom);
            event(&dom, upload, "click", Box::new(String::new()));
            let reset = settled(&mut dom).await;
            let input = replaced_input(&reset, input);
            let html = dioxus_ssr::render(&dom);
            assert!(html.contains("Logo uploaded and saved"));
            assert!(html.contains("Unsaved event"));
            assert!(!html.contains("Selected: logo.png"));
            choose(&dom, input);
            settled(&mut dom).await;
            assert_ready(&dom);
        },
    )
    .await;
    assert_eq!(requests.len(), 5);
    assert!(requests[1].starts_with("POST /v1/admin/files?location=conventions%2Fevent "));
    assert!(requests[2].ends_with("logo"));
    for request in &requests[3..] {
        assert!(request.starts_with("PATCH /v1/admin/conventions/"));
        assert!(request.ends_with(r#"{"logo_url":"conventions/event/key.png"}"#));
    }
}

#[tokio::test]
async fn saved_logo_has_replacement_action_without_manual_refresh_and_preserves_drafts() {
    let mut record = record();
    record.logo_preview_url = Some("https://s3.test/key.png?signature=old".into());
    record.logo_preview_url_expires_at = Some("2099-01-01T00:00:00Z".into());
    let ((), requests) = crate::api::tests::metadata_responses(&[(200, MEMBERSHIPS)], async {
        dioxus::html::set_event_converter(Box::new(FormEventConverter));
        let settings = consume_context::<crate::config::Settings>();
        let mut dom = VirtualDom::new(editor)
            .with_root_context(settings)
            .with_root_context(record);
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        let initial = dom.rebuild_to_vec();
        settled(&mut dom).await;
        form_event(
            &dom,
            listeners(&initial, "input")[0],
            "input",
            "Unsaved event",
        );
        settled(&mut dom).await;
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("signature=old"));
        assert!(html.contains("Unsaved event"));
        assert!(html.contains("Click image to replace"));
        assert!(!html.contains("Refresh logo preview"));
    })
    .await;
    assert_eq!(requests.len(), 1);
}
