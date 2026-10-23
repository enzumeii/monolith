use super::*;

#[test]
fn metadata_fields_keep_identity_readonly_and_date_inputs_are_calendar_dates() {
    fn app() -> Element {
        let value = Metadata {
            slug: "event".into(),
            currency: "THB".into(),
            methods: vec![ConventionPaymentMethod::Stripe],
            ..Default::default()
        };
        let draft = use_signal(|| value);
        rsx! { MetadataFields { draft, options: vec![("THB".into(), vec![ConventionPaymentMethod::Stripe])], editing: true } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("readonly"));
    assert!(
        html.contains("<option value=\"THB\" selected=true>THB</option>"),
        "{html}"
    );
    assert!(html.contains("<option value=\"\">Choose currency</option>"));
    assert!(html.contains("type=\"date\""));
    assert!(html.contains("Enable preorders"));
    assert!(html.contains("Description preview"));
    assert!(!html.contains("datetime-local"));
}
#[test]
fn unsupported_saved_capabilities_are_preserved_and_explained() {
    fn app() -> Element {
        let value = Metadata {
            currency: "JPY".into(),
            methods: vec![ConventionPaymentMethod::Paypal],
            ..Default::default()
        };
        let draft = use_signal(|| value);
        rsx! { MetadataFields { draft, options: Vec::new(), editing: true } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("<option value=\"JPY\" selected=true>JPY (currently unsupported)</option>"),
        "{html}"
    );
    assert!(html.contains("<option value=\"\">Choose currency</option>"));
    assert!(html.contains("PayPal"));
    assert!(html.contains("stored method is unsupported"));
    assert!(html.contains("You can edit metadata or disable preorders"));
    let methods = html
        .split("Payment methods")
        .nth(1)
        .expect("payment fieldset")
        .split("</fieldset>")
        .next()
        .expect("fieldset body");
    assert!(
        !methods.contains("disabled"),
        "stored unsupported method must remain removable"
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::{
        session,
        test_events::{FormEventConverter, form_event, listeners, render_changes},
    };
    use dioxus::history::{MemoryHistory, provide_history_context};
    use std::rc::Rc;
    const OPTIONS: &str = r#"{"currencies":[{"code":"THB","payment_methods":["stripe"]}]}"#;
    const UPLOADED: &str = r#"{"location":"conventions/event/key.png","url":"https://s3.test/key.png?signature=one","expires_at":"2026-11-01T00:05:00Z"}"#;
    const CREATED: &str = r##"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","slug":"event","name":"Event","logo_location":"conventions/event/key.png","logo_url":"https://s3.test/key.png?signature=one","logo_url_expires_at":"2026-11-01T00:05:00Z","country_code":"TH","event_starts_on":"2026-11-01","event_ends_on":"2026-11-02","description_markdown":"# Event","preorder_enabled":false,"currency_code":"THB","payment_methods":["stripe"],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"##;
    fn app() -> Element {
        use_hook(|| provide_history_context(Rc::new(MemoryHistory::with_initial_path("/create"))));
        rsx! { Router::<TestRoute> {} }
    }
    #[derive(Clone, PartialEq, Routable)]
    enum TestRoute {
        #[route("/create")]
        CreateEditor {},
    }
    async fn settle(dom: &mut VirtualDom) -> dioxus::dioxus_core::Mutations {
        let mut changes = render_changes(dom);
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            changes.edits.extend(render_changes(dom).edits);
        }
        changes
    }
    fn choose(dom: &VirtualDom, input: dioxus::dioxus_core::ElementId) {
        let file = crate::pages::conventions::logo::tests::LogoFixture("image/png");
        let data =
            dioxus::html::PlatformEventData::new(Box::new(vec![dioxus::html::FileData::new(file)]));
        dom.runtime().handle_event(
            "change",
            Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
            input,
        );
    }
    #[tokio::test]
    async fn creation_reuses_uploaded_location_after_post_failure_and_slug_change_uploads_again() {
        let new_upload = UPLOADED.replace("event/", "event-two/");
        let new_record = CREATED
            .replace("\"slug\":\"event\"", "\"slug\":\"event-two\"")
            .replace("conventions/event/", "conventions/event-two/");
        let ((), requests) = crate::api::tests::metadata_responses(
            &[
                (200, OPTIONS),
                (502, r#"{"error":"Upload failed"}"#),
                (201, UPLOADED),
                (409, r#"{"error":"Create failed"}"#),
                (409, r#"{"error":"Create failed again"}"#),
                (201, &new_upload),
                (201, &new_record),
            ],
            async {
                dioxus::html::set_event_converter(Box::new(FormEventConverter));
                let settings = consume_context::<crate::config::Settings>();
                let mut dom = VirtualDom::new(app).with_root_context(settings);
                dom.in_scope(ScopeId::ROOT, || {
                    *session::SESSION.write() = Some(session::Session {
                        admin_id: 1,
                        username: "fixture".into(),
                        role: domain::AdminRole::Admin,
                        expires_at: u64::MAX,
                    });
                });
                let initial = dom.rebuild_to_vec();
                settle(&mut dom).await;
                let inputs = listeners(&initial, "input");
                for (input, value) in inputs.iter().zip([
                    "Event",
                    "event",
                    "TH",
                    "2026-11-01",
                    "2026-11-02",
                    "# Event",
                ]) {
                    form_event(&dom, *input, "input", value);
                }
                let changes = listeners(&initial, "change");
                form_event(&dom, changes[1], "change", "THB");
                let method_render = settle(&mut dom).await;
                let stripe = *listeners(&method_render, "change")
                    .first()
                    .unwrap_or_else(|| {
                        panic!(
                            "no method: {} initial changes: {changes:?}",
                            dioxus_ssr::render(&dom)
                        )
                    });
                form_event(&dom, stripe, "change", "true");
                choose(&dom, changes[0]);
                settle(&mut dom).await;
                let submit = listeners(&initial, "submit")[0];
                for message in ["Upload failed", "Create failed", "Create failed again"] {
                    form_event(&dom, submit, "submit", "");
                    settle(&mut dom).await;
                    let html = dioxus_ssr::render(&dom);
                    assert!(html.contains(message), "{html}");
                    assert!(html.contains("Selected: logo.png"));
                    assert!(html.contains("value=\"Event\""));
                }
                form_event(&dom, inputs[1], "input", "event-two");
                settle(&mut dom).await;
                form_event(&dom, submit, "submit", "");
                settle(&mut dom).await;
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Convention created."), "{html}");
            },
        )
        .await;
        assert_eq!(requests.len(), 7);
        assert!(requests[1].starts_with("POST /v1/admin/files?location=conventions%2Fevent "));
        assert!(requests[2].starts_with("POST /v1/admin/files?location=conventions%2Fevent "));
        for request in &requests[3..5] {
            assert!(request.starts_with("POST /v1/admin/conventions "));
            assert!(request.contains(r#""logo_location":"conventions/event/key.png""#));
            assert!(!request.contains("signature"));
        }
        assert!(requests[5].starts_with("POST /v1/admin/files?location=conventions%2Fevent-two "));
        assert!(requests[6].contains(r#""logo_location":"conventions/event-two/key.png""#));
    }
}
