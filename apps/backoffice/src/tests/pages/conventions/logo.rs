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
use crate::test_events::{FormEventConverter, listeners, render_changes};
use std::rc::Rc;
#[test]
fn automatic_refresh_for_previous_revision_never_replaces_a_new_logo() {
    let record: ConventionDTO = serde_json::from_value(serde_json::json!({
        "id":"event", "slug":"event", "name":"Event", "country_code":"TH", "currency_code":"THB",
        "event_starts_on":"2026-11-01", "event_ends_on":"2026-11-02", "description_markdown":"Event",
        "preorder_enabled":false, "payment_methods":["stripe"], "created_at":"now", "updated_at":"now",
        "logo_preview_url":"https://s3.test/old.png", "logo_preview_url_expires_at":"2099-01-01T00:00:00Z"
    })).expect("convention fixture with signed logo preview");
    let mut preview = Preview::from_record(&record);
    let revision = preview.revision;
    let mut attached = record.clone();
    attached.logo_preview_url = Some("https://s3.test/new.png".into());
    preview.replace(&attached);
    preview.refresh(revision, &record);
    assert_eq!(preview.url, attached.logo_preview_url);
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
fn unavailable_preview_has_no_manual_refresh_and_saved_changes_advance_revision() {
    fn app() -> Element {
        let preview = use_signal(Preview::default);
        rsx! { LogoPreview { preview, id: "event" } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("No logo preview available"), "{html}");
    assert!(!html.contains("Refresh logo preview"));
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

#[tokio::test]
async fn selected_logo_has_an_image_preview_and_replacement_button() {
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(picker_app);
    let initial = dom.rebuild_to_vec();
    let data = dioxus::html::PlatformEventData::new(Box::new(vec![dioxus::html::FileData::new(
        LogoFixture("image/png"),
    )]));
    dom.runtime().handle_event(
        "change",
        Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
        listeners(&initial, "change")[0],
    );
    for _ in 0..30 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        render_changes(&mut dom);
    }
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("data:image/png;base64,"), "{html}");
    assert!(html.contains("Selected logo preview"));
    assert!(html.contains("Click image to replace"));
    assert!(html.contains("not yet saved"));
}
