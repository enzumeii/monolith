use super::{
    metadata::{Capabilities, MetadataFields},
    state::Metadata,
};
use crate::{
    api,
    components::{
        Confirm, DraftDepartureGuard, ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::{ConventionDTO, PatchConvention, PatchField};

#[component]
pub fn Detail(id: String) -> Element {
    let mut reload = use_signal(|| 0_u64);
    let request_id = id.clone();
    let record = use_resource(move || {
        let _ = reload();
        let id = request_id.clone();
        async move {
            let record = session::with_session(|| api::conventions::get(&id)).await?;
            let options = session::with_session(api::conventions::options).await?;
            Ok::<_, api::ApiError>((
                record,
                options
                    .currencies
                    .into_iter()
                    .map(|item| (item.code, item.payment_methods))
                    .collect::<Capabilities>(),
            ))
        }
    });
    rsx! {
        Link { to: Route::Conventions { query: String::new() }, "Back to conventions" }
        if record.read().is_none() || record.pending() { p { role: "status", "Loading convention…" } }
        else if let Some(result) = record.read().as_ref() {
            match result {
                Err(error) => rsx! { ErrorMessage { message: Some(error.to_string()) } button { onclick: move |_| reload += 1, "Retry loading" } },
                Ok((record, options)) => rsx! { Editor { key: "{id}:{reload}", record: record.clone(), options: options.clone(), on_reload: move |()| reload += 1 } },
            }
        }
    }
}
#[derive(Clone, PartialEq)]
struct LogoFile {
    name: String,
    mime: String,
    bytes: Vec<u8>,
}

#[component]
fn Editor(record: ConventionDTO, options: Capabilities, on_reload: EventHandler<()>) -> Element {
    let owner = use_owner();
    let mut draft = use_signal(|| Metadata::from_record(&record));
    let mut saved = use_signal(|| Metadata::from_record(&record));
    let mut logo = use_signal(|| record.logo_url.clone());
    let mut file = use_signal(|| None::<LogoFile>);
    let mut metadata_request = use_signal(RequestState::default);
    let mut logo_request = use_signal(RequestState::default);
    let mut metadata_error = use_signal(|| None::<String>);
    let mut logo_error = use_signal(|| None::<String>);
    let mut metadata_notice = use_signal(|| None::<String>);
    let mut logo_notice = use_signal(|| None::<String>);
    use_effect(move || {
        if draft() != saved() {
            metadata_notice.set(None);
        }
    });
    let mut selection_dirty = use_signal(|| false);
    let mut reload_confirmation = use_signal(|| false);
    let mut clear_confirmation = use_signal(|| false);
    let settings = use_context::<crate::config::Settings>();
    let current_logo =
        logo().and_then(|value| api::conventions::logo_url(&value, &settings.api.base_url));
    let dirty = draft() != saved()
        || file().is_some()
        || selection_dirty()
        || metadata_request.read().pending()
        || logo_request.read().pending();
    let id = record.id.clone();
    let upload_id = record.id.clone();
    let clear_id = record.id.clone();
    let allowed = !draft().requires_capabilities(&saved())
        || options
            .iter()
            .find(|(code, _)| code == &draft.read().currency)
            .is_some_and(|(_, methods)| {
                draft
                    .read()
                    .methods
                    .iter()
                    .all(|method| methods.contains(method))
            });

    rsx! {
        p { role: "status", "Saved preorders: " if saved.read().enabled { strong { "Enabled" } } else { strong { "Disabled" } } " · {record.slug} · {record.currency_code}" }
        p { "Enabled conventions with eligible selections are intended for the customer homepage. Event dates do not schedule availability. Saving or enabling validates the current selection on the server." }
        section { class: "panel convention-section", aria_label: "Convention metadata",
            h3 { "Convention metadata" }
            form { novalidate: true, onsubmit: move |event| {
                event.prevent_default(); if metadata_request.read().pending() { return; }
                if !allowed { metadata_error.set(Some("Restore currency and payment capabilities before enabling preorders or changing payment methods".into())); return; }
                let snapshot = draft(); if snapshot == saved() { return; } let input = match snapshot.patch(&saved()) { Ok(value) => value, Err(error) => { metadata_error.set(Some(error)); return; } };
                let Some(ticket) = metadata_request.write().begin() else { return; }; let id = id.clone(); let request_owner = owner();
                metadata_error.set(None); metadata_notice.set(None);
                spawn(async move {
                    let result = session::with_session(|| api::conventions::update(&id, input)).await;
                    if owner() != request_owner || !metadata_request.write().finish(ticket) { return; }
                    match result { Ok(record) => { let value = Metadata::from_record(&record); saved.set(value.clone()); if draft() == snapshot { draft.set(value); } metadata_notice.set(Some("Metadata saved".into())); }, Err(error) => metadata_error.set(Some(error.to_string())) }
                });
            },
                fieldset { disabled: metadata_request.read().pending(), MetadataFields { draft, options, editing: true } }
                ErrorMessage { message: metadata_error() }
                if let Some(message) = metadata_notice() { p { role: "status", "{message}" } }
                button { r#type: "submit", class: "primary", disabled: metadata_request.read().pending() || !allowed || draft() == saved(), if metadata_request.read().pending() { "Saving metadata…" } else { "Save metadata" } }
            }
        }
        section { class: "panel convention-section", aria_label: "Convention logo",
            h3 { "Convention logo" }
            p { "Upload saves the logo independently. PNG or JPEG only, at most 2 MiB and 4096 × 4096 pixels." }
            if let Some(url) = current_logo { img { class: "convention-logo", src: url, alt: "Saved convention logo" } }
            else { p { "No saved logo" } }
            label { r#for: "convention-logo-file", "Choose logo file" }
            input { id: "convention-logo-file", r#type: "file", accept: "image/png,image/jpeg", disabled: logo_request.read().pending(),
                onchange: move |event| {
                    if logo_request.read().pending() { return; }
                    file.set(None); logo_error.set(None); logo_notice.set(None);
                    let Some(selected) = event.files().into_iter().next() else { return; };
                    let mime = selected.content_type().unwrap_or_default();
                    if !matches!(mime.as_str(), "image/png" | "image/jpeg") || !(1..=2_097_152).contains(&selected.size()) { logo_error.set(Some("Choose a PNG or JPEG between 1 byte and 2 MiB".into())); return; }
                    let Some(ticket) = logo_request.write().begin() else { return; }; let request_owner = owner();
                    spawn(async move {
                        let result = selected.read_bytes().await;
                        if owner() != request_owner || !logo_request.write().finish(ticket) { return; }
                        match result { Ok(bytes) if (1..=2_097_152).contains(&bytes.len()) => file.set(Some(LogoFile { name: selected.name(), mime, bytes: bytes.to_vec() })), _ => logo_error.set(Some("Unable to read this logo file within the 2 MiB limit".into())) }
                    });
                }
            }
            if let Some(selected) = file() { p { "Selected: {selected.name} ({selected.bytes.len()} bytes), not yet saved" } }
            ErrorMessage { message: logo_error() }
            if let Some(message) = logo_notice() { p { role: "status", "{message}" } }
            div { class: "convention-actions",
                button { r#type: "button", class: "primary", disabled: logo_request.read().pending() || file().is_none(), onclick: move |_| {
                    let Some(selected) = file() else { return; }; let Some(ticket) = logo_request.write().begin() else { return; };
                    let id = upload_id.clone(); let request_owner = owner(); logo_error.set(None); logo_notice.set(None);
                    spawn(async move { let result = session::with_session(|| api::conventions::upload_logo(&id, &selected.mime, selected.bytes)).await;
                        if owner() != request_owner || !logo_request.write().finish(ticket) { return; }
                        match result { Ok(record) => { logo.set(record.logo_url); file.set(None); logo_notice.set(Some("Logo uploaded and saved".into())); }, Err(error) => logo_error.set(Some(error.to_string())) }
                    });
                }, if logo_request.read().pending() { "Processing logo…" } else { "Upload logo" } }
                button { r#type: "button", class: "ghost", disabled: logo_request.read().pending() || file().is_none(), onclick: move |_| { file.set(None); logo_error.set(None); }, "Discard selected file" }
                button { r#type: "button", class: "danger", disabled: logo_request.read().pending() || logo().is_none(), onclick: move |_| clear_confirmation.set(true), "Clear saved logo" }
            }
        }
        super::catalogue::SelectionEditor { id: record.id, currency: record.currency_code, enabled: saved.read().enabled, on_dirty: move |value| selection_dirty.set(value) }
        button { r#type: "button", class: "ghost", disabled: metadata_request.read().pending() || logo_request.read().pending(), onclick: move |_| reload_confirmation.set(true), "Reload saved convention" }
        DraftDepartureGuard { dirty, warning: "Your unsaved convention changes will be lost if you leave this page.".to_string() }
        Confirm { open: reload_confirmation(), title: "Reload saved convention?", message: "Reloading replaces all unsaved metadata, prices and selected files with saved data.", confirm_label: "Reload convention", danger: dirty, on_cancel: move |()| reload_confirmation.set(false), on_confirm: move |()| { reload_confirmation.set(false); on_reload(()); } }
        Confirm { open: clear_confirmation(), title: "Clear saved logo?", message: "The current logo reference will be removed. Previously uploaded objects are retained in storage.", confirm_label: "Clear logo", danger: true, on_cancel: move |()| clear_confirmation.set(false), on_confirm: move |()| {
            clear_confirmation.set(false); let Some(ticket) = logo_request.write().begin() else { return; }; let id = clear_id.clone(); let request_owner = owner(); logo_error.set(None); logo_notice.set(None);
            spawn(async move { let result = session::with_session(|| api::conventions::update(&id, PatchConvention { logo_url: PatchField::Clear, ..Default::default() })).await;
                if owner() != request_owner || !logo_request.write().finish(ticket) { return; }
                match result { Ok(record) => { logo.set(record.logo_url); logo_notice.set(Some("Saved logo cleared".into())); }, Err(error) => logo_error.set(Some(error.to_string())) }
            });
        } }
    }
}
