use super::{logo, metadata};
use crate::{
    api,
    components::{DraftDepartureGuard, ErrorMessage, Heading, request_state::RequestState},
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::{ConventionDTO, ConventionOptionsResponse};

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
            Heading { title: "Convention details", back: rsx! { Link { to: Route::Conventions { query: String::new() }, class: "back-link", aria_label: "Back to conventions", "<" } } }
            p { role: "status", "Loading convention…" }
        }
        else if let Some(result) = record.read().as_ref() {
            match result {
                Err(error) => rsx! { Heading { title: "Convention details", back: rsx! { Link { to: Route::Conventions { query: String::new() }, class: "back-link", aria_label: "Back to conventions", "<" } } } ErrorMessage { message: Some(error.to_string()) } button { class: "ghost", onclick: move |_| reload += 1, "Retry loading" } },
                Ok((record, options)) => rsx! { Editor { key: "{id}:{reload}", record: record.clone(), options: options.clone() } },
            }
        }
    }
}
#[component]
fn Editor(record: ConventionDTO, options: ConventionOptionsResponse) -> Element {
    let mut deleted = use_signal(|| false);
    let delete_request = use_signal(RequestState::default);
    let metadata = metadata::use_metadata_editor(&record);
    let logo = logo::use_logo_editor(&record);
    let mut selection_dirty = use_signal(|| false);
    let dirty = metadata.dirty() || logo.dirty() || selection_dirty();
    let saving = metadata.request.read().pending() || logo.request.read().pending();

    rsx! {
        if deleted() {
            super::navigation::ReturnToList { message: "Convention deleted. Returning to conventions…" }
        } else {
            Heading { title: metadata.saved.read().name.clone(),
                back: rsx! { Link { to: Route::Conventions { query: String::new() }, class: "back-link", aria_label: "Back to conventions", title: "Back to conventions", "<" } },
                super::delete::DeleteConvention {
                    id: record.id.clone(), name: metadata.saved.read().name.clone(),
                    request: delete_request, disabled: saving, on_deleted: move |()| deleted.set(true),
                }
            }
            fieldset { disabled: delete_request.read().pending(),
                metadata::EditMetadata { id: record.id.clone(), options, editor: metadata }
                logo::EditLogo { id: record.id.clone(), slug: record.slug, editor: logo, delete_pending: delete_request.read().pending() }
                super::catalogue::SelectionEditor {
                    id: record.id.clone(), currency: record.currency_code, enabled: metadata.saved.read().enabled,
                    on_dirty: move |value| selection_dirty.set(value),
                }
            }
            DraftDepartureGuard { dirty, warning: "Your unsaved convention changes will be lost if you leave this page.".to_string() }
            logo::ClearLogoConfirmation { id: record.id, editor: logo }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../../tests/pages/conventions/detail.rs"]
mod tests;
