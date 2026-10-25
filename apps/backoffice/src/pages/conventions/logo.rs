use crate::{
    api,
    components::{
        Confirm, ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    session,
};
use base64::Engine;
use dioxus::{html::FileData, prelude::*};
use domain::dto::{ConventionDTO, PatchConvention, PatchField};

#[derive(Clone, PartialEq)]
pub(super) struct LogoFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    preview_url: String,
}

impl LogoFile {
    async fn read(selected: FileData, mime: String) -> Result<Self, String> {
        let bytes = selected
            .read_bytes()
            .await
            .map_err(|_| "Unable to read this logo file within the 2 MiB limit".to_string())?;
        if !(1..=2_097_152).contains(&bytes.len()) {
            return Err("Unable to read this logo file within the 2 MiB limit".into());
        }
        Ok(Self {
            name: selected.name(),
            preview_url: format!(
                "data:{};base64,{}",
                mime,
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            ),
            mime,
            bytes: bytes.to_vec(),
        })
    }
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
        let _loading = ui::loading_guard();
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

#[derive(Clone, Copy, PartialEq)]
pub(super) struct LogoEditor {
    preview: Signal<Preview>,
    selection: LogoSelection,
    pub request: Signal<RequestState>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    confirmation: Signal<bool>,
    owner: Memo<Option<(i64, domain::AdminRole)>>,
}

impl LogoEditor {
    pub fn dirty(self) -> bool {
        (self.selection.file)().is_some() || self.request.read().pending()
    }

    pub fn upload(mut self, id: String, slug: String) {
        if (self.selection.file)().is_none() {
            return;
        }
        let Some(ticket) = self.request.write().begin() else {
            return;
        };
        let request_owner = (self.owner)();
        self.error.set(None);
        self.notice.set(None);
        spawn(async move {
            let _loading = ui::loading_guard();
            let result = async {
                let location = self.selection.upload(&slug).await?;
                if (self.owner)() != request_owner {
                    return Err(api::ApiError::new(
                        reqwest::StatusCode::CONFLICT,
                        "Session changed",
                    ));
                }
                self.selection.staged.set(Some((slug, location.clone())));
                session::with_session(|| {
                    api::conventions::update(
                        &id,
                        PatchConvention {
                            logo_url: PatchField::Set(location),
                            ..Default::default()
                        },
                    )
                })
                .await
            }
            .await;
            if (self.owner)() != request_owner || !self.request.write().finish(ticket) {
                return;
            }
            match result {
                Ok(record) => {
                    self.preview.write().replace(&record);
                    self.selection.reset();
                    self.notice.set(Some("Logo uploaded and saved".into()));
                }
                Err(error) => self.error.set(Some(error.to_string())),
            }
        });
    }

    pub fn clear(mut self, id: String) {
        self.confirmation.set(false);
        let Some(ticket) = self.request.write().begin() else {
            return;
        };
        let request_owner = (self.owner)();
        self.error.set(None);
        self.notice.set(None);
        spawn(async move {
            let _loading = ui::loading_guard();
            let result = session::with_session(|| {
                api::conventions::update(
                    &id,
                    PatchConvention {
                        logo_url: PatchField::Clear,
                        ..Default::default()
                    },
                )
            })
            .await;
            if (self.owner)() != request_owner || !self.request.write().finish(ticket) {
                return;
            }
            match result {
                Ok(record) => {
                    self.preview.write().replace(&record);
                    self.notice.set(Some("Saved logo cleared".into()));
                }
                Err(error) => self.error.set(Some(error.to_string())),
            }
        });
    }
}

pub(super) fn use_logo_editor(record: &ConventionDTO) -> LogoEditor {
    LogoEditor {
        preview: use_signal(|| Preview::from_record(record)),
        selection: use_logo_selection(),
        request: use_signal(RequestState::default),
        error: use_signal(|| None),
        notice: use_signal(|| None),
        confirmation: use_signal(|| false),
        owner: use_owner(),
    }
}

#[component]
pub(super) fn EditLogo(
    id: String,
    slug: String,
    mut editor: LogoEditor,
    delete_pending: bool,
) -> Element {
    let pending = editor.request.read().pending();
    rsx! {
        section { class: "panel convention-section", aria_label: "Convention logo",
            h3 { "Convention logo" }
            p { "Upload saves the logo independently. PNG or JPEG only, at most 2 MiB and 4096 × 4096 pixels." }
            LogoPreview { preview: editor.preview, id: id.clone(), replace_disabled: pending || delete_pending }
            LogoPicker { selection: editor.selection, request: editor.request, error: editor.error, notice: editor.notice }
            ErrorMessage { message: (editor.error)() }
            if let Some(message) = (editor.notice)() { p { role: "status", "{message}" } }
            div { class: "convention-actions",
                button { id: "convention-logo-upload", r#type: "button", class: "primary", disabled: pending || (editor.selection.file)().is_none(),
                    onclick: move |_| editor.upload(id.clone(), slug.clone()),
                    if pending { "Processing logo…" } else { "Upload logo" }
                }
                button { id: "convention-logo-clear", r#type: "button", class: "danger", disabled: pending,
                    onclick: move |_| editor.confirmation.set(true), "Clear saved logo"
                }
            }
        }
    }
}

#[component]
pub(super) fn ClearLogoConfirmation(id: String, mut editor: LogoEditor) -> Element {
    rsx! {
        Confirm { open: (editor.confirmation)(), title: "Clear saved logo?",
            message: "The current logo reference will be removed. Previously uploaded objects are retained in storage.",
            confirm_label: "Clear logo", danger: true,
            on_cancel: move |()| editor.confirmation.set(false),
            on_confirm: move |()| editor.clear(id.clone()),
        }
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
        div { class: "convention-logo-file-box",
        label { class: "convention-label", r#for: "convention-logo-file", "Choose logo file" }
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
                        let loading = ui::loading_guard();
                        loading.paint().await;
                        let result = LogoFile::read(selected, mime).await;
                        if owner() != request_owner || !request.write().finish(ticket) { return; }
                        match result {
                            Ok(file) => selection.file.set(Some(file)),
                            Err(message) => error.set(Some(message)),
                        }
                    });
                }
            }
        }
        if let Some(selected) = (selection.file)() {
            div { class: "convention-logo-candidate",
                LogoImageButton { src: selected.preview_url, alt: "Selected logo preview", disabled: request.read().pending(), on_error: move |()| error.set(Some("Unable to preview this image. Choose a valid PNG or JPEG.".into())) }
                p { "Selected: {selected.name} (not yet saved)" }
            }
        }
        button { id: "convention-logo-discard", r#type: "button", class: "ghost convention-file-discard", disabled: request.read().pending() || (selection.file)().is_none(), onclick: move |_| { selection.reset(); error.set(None); }, "Discard selected file" }
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Preview {
    revision: u64,
    url: Option<String>,
    expires_at: Option<String>,
    failed: bool,
}
impl Preview {
    pub fn from_record(record: &ConventionDTO) -> Self {
        Self {
            revision: 0,
            url: record.logo_preview_url.clone(),
            expires_at: record.logo_preview_url_expires_at.clone(),
            failed: false,
        }
    }
    pub fn replace(&mut self, record: &ConventionDTO) {
        *self = Self {
            revision: self.revision.wrapping_add(1),
            ..Self::from_record(record)
        };
    }
    fn refresh(&mut self, revision: u64, record: &ConventionDTO) {
        if self.revision == revision {
            self.replace(record);
        }
    }
}

#[component]
pub(super) fn LogoPreview(
    mut preview: Signal<Preview>,
    id: String,
    #[props(default)] replace_disabled: bool,
) -> Element {
    let owner = use_owner();
    let mut timer = use_signal(|| None::<document::Eval>);
    let mut error = use_signal(|| None::<String>);
    use_future(move || {
        let id = id.clone();
        async move {
            let mut bridge = document::eval(include_str!("logo_refresh.js"));
            timer.set(Some(bridge));
            while let Ok(true) = bridge.recv::<bool>().await {
                let revision = preview.peek().revision;
                let request_owner = owner();
                let result = session::with_session(|| api::conventions::get(&id)).await;
                if owner() != request_owner {
                    break;
                }
                if preview.peek().revision != revision {
                    continue;
                }
                match result {
                    Ok(record) => {
                        preview.write().refresh(revision, &record);
                        error.set(None);
                    }
                    Err(value) => {
                        error.set(Some(value.to_string()));
                        let _ = bridge.send((preview.peek().expires_at.clone(), true));
                    }
                }
            }
        }
    });
    use_effect(move || {
        let current = preview();
        if let Some(bridge) = timer() {
            let _ = bridge.send((current.expires_at, current.failed));
        }
    });
    use_drop(move || {
        if let Some(bridge) = *timer.peek() {
            let _ = bridge.send(None::<bool>);
        }
    });
    let current = preview();
    let url = current
        .url
        .as_deref()
        .and_then(api::conventions::logo_preview_url);
    let unavailable = current.failed || url.is_none() || current.expires_at.is_none();
    rsx! {
        if !unavailable { if let Some(url) = url { LogoImageButton { src: url, alt: "Saved convention logo", disabled: replace_disabled, on_error: move |()| preview.write().failed = true } } }
        else { p { role: "status", "No logo preview available." } }
        ErrorMessage { message: error() }
    }
}

#[component]
fn LogoImageButton(
    src: String,
    alt: String,
    disabled: bool,
    on_error: EventHandler<()>,
) -> Element {
    rsx! {
        button { r#type: "button", class: "ghost convention-logo-replace", disabled, aria_label: "{alt}. Choose a replacement logo", onclick: move |_| {
            let _ = document::eval("document.getElementById('convention-logo-file')?.click();");
        },
            img { class: "convention-logo", src, alt, onerror: move |_| on_error(()) }
            span { "Click image to replace" }
        }
    }
}

#[component]
pub(super) fn LogoThumbnail(
    name: String,
    preview_url: Option<String>,
    expires_at: Option<String>,
) -> Element {
    let mut failed = use_signal(|| false);
    let url = preview_url
        .as_deref()
        .and_then(api::conventions::logo_preview_url);
    let fresh = expires_at
        .as_deref()
        .and_then(|expiry| chrono::DateTime::parse_from_rfc3339(expiry).ok())
        .is_some_and(|expiry| expiry.timestamp_millis() > chrono::Utc::now().timestamp_millis());
    rsx! {
        if !failed() && fresh {
            if let Some(url) = url {
                img { class: "convention-thumbnail", src: url, alt: "Logo for {name}", width: "56", height: "56", loading: "lazy", onerror: move |_| failed.set(true) }
            } else { span { class: "convention-thumbnail-placeholder convention-label", "No logo preview" } }
        } else { span { class: "convention-thumbnail-placeholder convention-label", "No logo preview" } }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../../tests/pages/conventions/logo.rs"]
pub(super) mod tests;
