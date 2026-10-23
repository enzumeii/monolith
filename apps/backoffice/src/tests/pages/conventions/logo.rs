use dioxus::dioxus_core::CapturedError;
pub(crate) struct LogoFixture(pub &'static str);
impl dioxus::html::NativeFileData for LogoFixture {
    fn name(&self) -> String {
        "logo.png".into()
    }
    fn size(&self) -> u64 {
        4
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn path(&self) -> std::path::PathBuf {
        "logo.png".into()
    }
    fn content_type(&self) -> Option<String> {
        Some(self.0.into())
    }
    fn read_bytes(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<dioxus::html::bytes::Bytes, CapturedError>>>,
    > {
        Box::pin(async { Ok(dioxus::html::bytes::Bytes::from_static(b"logo")) })
    }
    fn read_string(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, CapturedError>>>> {
        Box::pin(async { Ok("logo".into()) })
    }
    fn byte_stream(
        &self,
    ) -> std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<dioxus::html::bytes::Bytes, CapturedError>> + Send>,
    > {
        Box::pin(futures::stream::once(async {
            Ok(dioxus::html::bytes::Bytes::from_static(b"logo"))
        }))
    }
    fn inner(&self) -> &dyn std::any::Any {
        self
    }
}

use super::*;
use crate::test_events::{FormEventConverter, form_event, listeners, render_changes};
use std::{cell::RefCell, rc::Rc};
type PreviewSignal = Rc<RefCell<Option<Signal<Preview>>>>;
fn preview_app() -> Element {
    let preview = use_signal(|| Preview {
        revision: 0,
        url: Some("https://s3.test/old.png?signature=old".into()),
        expires_at: Some("2026-11-01T00:05:00Z".into()),
        failed: false,
    });
    let captured = use_context::<PreviewSignal>();
    *captured.borrow_mut() = Some(preview);
    rsx! { LogoPreview { preview, id: "da41d46b-f1ca-4a76-a774-62c269d28731" } }
}

#[tokio::test]
async fn refresh_response_for_previous_revision_never_replaces_a_newly_attached_logo() {
    let body = r##"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","slug":"event","name":"Event","logo_preview_url":"https://s3.test/old.png?signature=refreshed","logo_preview_url_expires_at":"2026-11-01T00:10:00Z","country_code":"TH","event_starts_on":"2026-11-01","event_ends_on":"2026-11-02","description_markdown":"# Event","preorder_enabled":false,"currency_code":"THB","payment_methods":["stripe"],"created_at":"now","updated_at":"now"}"##;
    let ((), requests) = crate::api::tests::metadata_responses(&[(200, body)], async {
        dioxus::html::set_event_converter(Box::new(FormEventConverter));
        let captured = PreviewSignal::default();
        let settings = consume_context::<crate::config::Settings>();
        let mut dom = VirtualDom::new(preview_app)
            .with_root_context(settings)
            .with_root_context(captured.clone());
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        let initial = dom.rebuild_to_vec();
        form_event(&dom, listeners(&initial, "click")[0], "click", "");
        let mut preview = captured.borrow().expect("captured");
        let mut attached: ConventionDTO = serde_json::from_str(body).expect("convention response");
        attached.logo_preview_url = Some("https://s3.test/new.png?signature=new".into());
        attached.logo_preview_url_expires_at = Some("2026-11-01T00:20:00Z".into());
        preview.write().replace(&attached);
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            render_changes(&mut dom);
        }
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("new.png?signature=new"), "{html}");
        assert!(!html.contains("signature=refreshed"));
        assert!(html.contains("00:20:00Z"));
    })
    .await;
    assert_eq!(requests.len(), 1);
}

fn picker_app() -> Element {
    let selection = use_logo_selection();
    let request = use_signal(RequestState::default);
    let error = use_signal(|| None::<String>);
    let notice = use_signal(|| None::<String>);
    rsx! { LogoPicker { selection, request, error, notice } ErrorMessage { message: error() } }
}
#[test]
fn invalid_image_type_does_not_create_an_upload_candidate() {
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(picker_app);
    let initial = dom.rebuild_to_vec();
    let data = dioxus::html::PlatformEventData::new(Box::new(vec![dioxus::html::FileData::new(
        LogoFixture("image/svg+xml"),
    )]));
    dom.runtime().handle_event(
        "change",
        Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
        listeners(&initial, "change")[0],
    );
    render_changes(&mut dom);
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("Choose a PNG or JPEG between 1 byte and 2 MiB"));
    assert!(!html.contains("Selected: logo.png"));
}

#[test]
fn thumbnail_renders_only_safe_unexpired_signed_preview_urls() {
    #[component]
    fn app() -> Element {
        rsx! {
            LogoThumbnail { name: "Safe", preview_url: Some("https://s3.test/logo.png?signature=one".into()), expires_at: Some("2099-01-01T00:00:00Z".into()) }
            LogoThumbnail { name: "Expired", preview_url: Some("https://s3.test/expired.png".into()), expires_at: Some("2000-01-01T00:00:00Z".into()) }
            LogoThumbnail { name: "Unsafe", preview_url: Some("javascript:alert(1)".into()), expires_at: Some("2099-01-01T00:00:00Z".into()) }
            LogoThumbnail { name: "Missing", preview_url: None, expires_at: None }
        }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("Logo for Safe"), "{html}");
    assert!(html.contains("width=\"56\""));
    assert!(html.contains("height=\"56\""));
    assert!(html.contains("loading=\"lazy\""));
    assert!(!html.contains("expired.png"));
    assert!(!html.contains("javascript:"));
    assert_eq!(html.matches("No logo preview").count(), 3);
}

#[test]
fn unavailable_preview_retains_refresh_and_saved_changes_advance_revision() {
    fn app() -> Element {
        let preview = use_signal(Preview::default);
        rsx! { LogoPreview { preview, id: "event" } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("No logo preview available"), "{html}");
    assert!(html.contains("Refresh logo preview"));
    assert!(!html.contains("No saved logo"));
    let record: ConventionDTO = serde_json::from_value(serde_json::json!({
        "id": "event", "slug": "event", "name": "Event",
        "logo_preview_url": null, "logo_preview_url_expires_at": null,
        "country_code": "TH", "event_starts_on": "2026-11-01", "event_ends_on": "2026-11-02",
        "description_markdown": "", "preorder_enabled": false, "currency_code": "THB",
        "payment_methods": ["stripe"], "created_at": "now", "updated_at": "now"
    }))
    .expect("convention response");
    let mut preview = Preview::from_record(&record);
    let previous_revision = preview.revision;
    preview.replace(&record);
    assert_ne!(preview.revision, previous_revision);
    assert!(preview.url.is_none());
}
