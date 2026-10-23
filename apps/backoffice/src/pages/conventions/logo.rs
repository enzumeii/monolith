use crate::{
    api,
    components::{
        ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    session,
};
use dioxus::prelude::*;
use domain::dto::ConventionDTO;

#[derive(Clone, PartialEq)]
pub(super) struct LogoFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct LogoSelection {
    pub file: Signal<Option<LogoFile>>,
    pub staged: Signal<Option<(String, String)>>,
    generation: Signal<u64>,
}
impl LogoSelection {
    pub fn reset(mut self) {
        self.file.set(None);
        self.staged.set(None);
        self.generation += 1;
    }
    pub async fn upload(self, slug: &str) -> api::ApiResult<String> {
        if let Some((saved_slug, location)) = (self.staged)()
            && saved_slug == slug
        {
            return Ok(location);
        }
        let selected = (self.file)().ok_or_else(|| {
            api::ApiError::new(reqwest::StatusCode::BAD_REQUEST, "Choose a logo file")
        })?;
        let folder = format!("conventions/{slug}");
        let uploaded =
            session::with_session(|| api::files::upload(&folder, &selected.mime, selected.bytes))
                .await?;
        Ok(uploaded.location)
    }
}

pub(super) fn use_logo_selection() -> LogoSelection {
    LogoSelection {
        file: use_signal(|| None),
        staged: use_signal(|| None),
        generation: use_signal(|| 0),
    }
}

#[component]
pub(super) fn LogoPicker(
    mut selection: LogoSelection,
    mut request: Signal<RequestState>,
    mut error: Signal<Option<String>>,
    mut notice: Signal<Option<String>>,
) -> Element {
    let owner = use_owner();
    rsx! {
        label { r#for: "convention-logo-file", "Choose logo file" }
        for generation in [(selection.generation)()] {
            input { key: "{generation}", id: "convention-logo-file", r#type: "file", accept: "image/png,image/jpeg", disabled: request.read().pending(),
                onchange: move |event| {
                    if request.read().pending() { return; }
                    selection.file.set(None); selection.staged.set(None); error.set(None); notice.set(None);
                    let Some(selected) = event.files().into_iter().next() else { return; };
                    let mime = selected.content_type().unwrap_or_default();
                    if !matches!(mime.as_str(), "image/png" | "image/jpeg") || !(1..=2_097_152).contains(&selected.size()) { error.set(Some("Choose a PNG or JPEG between 1 byte and 2 MiB".into())); return; }
                    let Some(ticket) = request.write().begin() else { return; }; let request_owner = owner();
                    spawn(async move {
                        let result = selected.read_bytes().await;
                        if owner() != request_owner || !request.write().finish(ticket) { return; }
                        match result { Ok(bytes) if (1..=2_097_152).contains(&bytes.len()) => selection.file.set(Some(LogoFile { name: selected.name(), mime, bytes: bytes.to_vec() })), _ => error.set(Some("Unable to read this logo file within the 2 MiB limit".into())) }
                    });
                }
            }
        }
        if let Some(selected) = (selection.file)() { p { "Selected: {selected.name} ({selected.bytes.len()} bytes), not yet saved" } }
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Preview {
    pub location: Option<String>,
    url: Option<String>,
    expires_at: Option<String>,
    failed: bool,
}
impl Preview {
    pub fn from_record(record: &ConventionDTO) -> Self {
        Self {
            location: record.logo_url.clone(),
            url: record.logo_preview_url.clone(),
            expires_at: record.logo_preview_url_expires_at.clone(),
            failed: false,
        }
    }
}

#[component]
pub(super) fn LogoPreview(mut preview: Signal<Preview>, id: String) -> Element {
    let owner = use_owner();
    let mut request = use_signal(RequestState::default);
    let mut error = use_signal(|| None::<String>);
    let current = preview();
    let url = current
        .url
        .as_deref()
        .and_then(api::conventions::logo_preview_url);
    let unavailable = current.failed || url.is_none() || current.expires_at.is_none();
    rsx! {
        if current.location.is_none() { p { "No saved logo" } }
        else {
            if !unavailable { if let Some(url) = url { img { class: "convention-logo", src: url, alt: "Saved convention logo", onerror: move |_| preview.write().failed = true } } }
            else { p { role: "status", "Preview unavailable or expired. Refresh logo preview." } }
            if let Some(expiry) = current.expires_at { p { "Preview URL expires at {expiry}" } }
            ErrorMessage { message: error() }
            button { id: format!("convention-logo-refresh-{id}"), r#type: "button", class: "ghost", disabled: request.read().pending(), onclick: move |_| {
                let Some(ticket) = request.write().begin() else { return; };
                let location = preview.read().location.clone(); let request_owner = owner(); let id = id.clone(); error.set(None);
                spawn(async move {
                    let result = session::with_session(|| api::conventions::get(&id)).await;
                    if owner() != request_owner || !request.write().finish(ticket) || preview.read().location != location { return; }
                    match result {
                        Ok(record) if record.logo_url == location => { preview.set(Preview::from_record(&record)); },
                        Ok(_) => error.set(Some("The saved logo changed. Reload the saved convention when your drafts are ready.".into())),
                        Err(value) => error.set(Some(value.to_string())),
                    }
                });
            }, "Refresh logo preview" }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../../tests/pages/conventions/logo.rs"]
pub(super) mod tests;
