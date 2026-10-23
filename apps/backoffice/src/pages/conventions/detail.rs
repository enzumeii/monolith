use super::{
    logo::{LogoPicker, LogoPreview, Preview, use_logo_selection},
    metadata::MetadataFields,
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
use domain::dto::{ConventionDTO, ConventionOptionsResponse, PatchConvention, PatchField};

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
            Ok::<_, api::ApiError>((record, options))
        }
    });
    rsx! {
        if record.read().is_none() || record.pending() {
            h2 { class: "convention-editor-heading", Link { to: Route::Conventions { query: String::new() }, class: "button-link ghost convention-back", aria_label: "Back to conventions", "<" } "Convention details" }
            p { role: "status", "Loading convention…" }
        }
        else if let Some(result) = record.read().as_ref() {
            match result {
                Err(error) => rsx! { h2 { class: "convention-editor-heading", Link { to: Route::Conventions { query: String::new() }, class: "button-link ghost convention-back", aria_label: "Back to conventions", "<" } "Convention details" } ErrorMessage { message: Some(error.to_string()) } button { class: "ghost", onclick: move |_| reload += 1, "Retry loading" } },
                Ok((record, options)) => rsx! { Editor { key: "{id}:{reload}", record: record.clone(), options: options.clone(), on_reload: move |()| reload += 1 } },
            }
        }
    }
}
#[component]
fn Editor(
    record: ConventionDTO,
    options: ConventionOptionsResponse,
    on_reload: EventHandler<()>,
) -> Element {
    let owner = use_owner();
    let mut deleted = use_signal(|| false);
    let delete_request = use_signal(RequestState::default);
    let mut draft = use_signal(|| Metadata::from_record(&record));
    let mut saved = use_signal(|| Metadata::from_record(&record));
    let mut logo = use_signal(|| Preview::from_record(&record));
    let mut selection = use_logo_selection();
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
    let dirty = draft() != saved()
        || (selection.file)().is_some()
        || selection_dirty()
        || metadata_request.read().pending()
        || logo_request.read().pending();
    let id = record.id.clone();
    let upload_id = record.id.clone();
    let upload_slug = record.slug.clone();
    let clear_id = record.id.clone();
    let allowed = !draft().requires_capabilities(&saved())
        || options
            .currencies
            .iter()
            .find(|item| item.code == draft.read().currency)
            .is_some_and(|item| {
                draft
                    .read()
                    .methods
                    .iter()
                    .all(|method| item.payment_methods.contains(method))
            });

    rsx! {
        if deleted() {
            super::navigation::ReturnToList { message: "Convention deleted. Returning to conventions…" }
        } else {
        header { class: "convention-list-heading",
            h2 { class: "convention-editor-heading",
                Link { to: Route::Conventions { query: String::new() }, class: "button-link ghost convention-back", aria_label: "Back to conventions", title: "Back to conventions", "<" }
                span { "{saved.read().name}" }
            }
            super::delete::DeleteConvention { id: record.id.clone(), name: saved.read().name.clone(), request: delete_request, disabled: metadata_request.read().pending() || logo_request.read().pending(), on_deleted: move |()| deleted.set(true) }
        }
        fieldset { disabled: delete_request.read().pending(),
        section { class: "panel convention-section", aria_label: "Convention metadata",
            h3 { "Convention metadata" }
            form { novalidate: true, onsubmit: move |event| {
                event.prevent_default(); if metadata_request.read().pending() { return; }
                if !allowed { metadata_error.set(Some("Choose supported currency and payment methods before enabling preorders or changing payment methods".into())); return; }
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
            LogoPreview { preview: logo, id: record.id.clone(), replace_disabled: logo_request.read().pending() || delete_request.read().pending() }
            div { class: "convention-logo-file-box",
                LogoPicker { selection, request: logo_request, error: logo_error, notice: logo_notice }
                button { id: "convention-logo-discard", r#type: "button", class: "ghost convention-file-discard", disabled: logo_request.read().pending() || (selection.file)().is_none(), onclick: move |_| { selection.reset(); logo_error.set(None); }, "Discard selected file" }
            }
            ErrorMessage { message: logo_error() }
            if let Some(message) = logo_notice() { p { role: "status", "{message}" } }
            div { class: "convention-actions",
                button { id: "convention-logo-upload", r#type: "button", class: "primary", disabled: logo_request.read().pending() || (selection.file)().is_none(), onclick: move |_| {
                    if (selection.file)().is_none() { return; } let Some(ticket) = logo_request.write().begin() else { return; };
                    let id = upload_id.clone(); let slug = upload_slug.clone(); let request_owner = owner(); logo_error.set(None); logo_notice.set(None);
                    spawn(async move { let result = async {
                            let location = selection.upload(&slug).await?;
                            if owner() != request_owner { return Err(api::ApiError::new(reqwest::StatusCode::CONFLICT, "Session changed")); }
                            selection.staged.set(Some((slug, location.clone())));
                            session::with_session(|| api::conventions::update(&id, PatchConvention { logo_url: PatchField::Set(location), ..Default::default() })).await
                        }.await;
                        if owner() != request_owner || !logo_request.write().finish(ticket) { return; }
                        match result { Ok(record) => { logo.write().replace(&record); selection.reset(); logo_notice.set(Some("Logo uploaded and saved".into())); }, Err(error) => logo_error.set(Some(error.to_string())) }
                    });
                }, if logo_request.read().pending() { "Processing logo…" } else { "Upload logo" } }
                button { id: "convention-logo-clear", r#type: "button", class: "danger", disabled: logo_request.read().pending(), onclick: move |_| clear_confirmation.set(true), "Clear saved logo" }
            }
        }
        super::catalogue::SelectionEditor { id: record.id, currency: record.currency_code, enabled: saved.read().enabled, on_dirty: move |value| selection_dirty.set(value) }
        button { r#type: "button", class: "ghost", disabled: metadata_request.read().pending() || logo_request.read().pending(), onclick: move |_| reload_confirmation.set(true), "Reload saved convention" }
        }
        DraftDepartureGuard { dirty, warning: "Your unsaved convention changes will be lost if you leave this page.".to_string() }
        Confirm { open: reload_confirmation(), title: "Reload saved convention?", message: "Reloading replaces all unsaved metadata, prices and selected files with saved data.", confirm_label: "Reload convention", danger: dirty, on_cancel: move |()| reload_confirmation.set(false), on_confirm: move |()| { reload_confirmation.set(false); on_reload(()); } }
        Confirm { open: clear_confirmation(), title: "Clear saved logo?", message: "The current logo reference will be removed. Previously uploaded objects are retained in storage.", confirm_label: "Clear logo", danger: true, on_cancel: move |()| clear_confirmation.set(false), on_confirm: move |()| {
            clear_confirmation.set(false); let Some(ticket) = logo_request.write().begin() else { return; }; let id = clear_id.clone(); let request_owner = owner(); logo_error.set(None); logo_notice.set(None);
            spawn(async move { let result = session::with_session(|| api::conventions::update(&id, PatchConvention { logo_url: PatchField::Clear, ..Default::default() })).await;
                if owner() != request_owner || !logo_request.write().finish(ticket) { return; }
                match result { Ok(record) => { logo.write().replace(&record); logo_notice.set(Some("Saved logo cleared".into())); }, Err(error) => logo_error.set(Some(error.to_string())) }
            });
        } }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../../tests/pages/conventions/detail.rs"]
mod tests;
