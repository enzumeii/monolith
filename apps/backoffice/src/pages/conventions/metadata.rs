use super::{
    logo::{LogoPicker, use_logo_selection},
    state::{Metadata, method_label},
};
use crate::{
    api,
    components::{
        DraftDepartureGuard, ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::ConventionPaymentMethod;

pub type Capabilities = Vec<(String, Vec<ConventionPaymentMethod>)>;

#[component]
pub fn CreateEditor() -> Element {
    let draft = use_signal(Metadata::default);
    let owner = use_owner();
    let mut selection = use_logo_selection();
    let logo_request = use_signal(RequestState::default);
    let notice = use_signal(|| None::<String>);
    use_effect(move || {
        let slug = draft.read().slug.clone();
        if selection
            .staged
            .read()
            .as_ref()
            .is_some_and(|(saved, _)| saved != &slug)
        {
            selection.staged.set(None);
        }
    });
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut created = use_signal(|| None::<String>);
    let mut retry = use_signal(|| 0_u64);
    let capabilities = use_resource(move || {
        let _ = retry();
        async move { session::with_session(api::conventions::options).await }
    });
    let options = capabilities
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|value| {
            value
                .currencies
                .iter()
                .map(|item| (item.code.clone(), item.payment_methods.clone()))
                .collect::<Capabilities>()
        })
        .unwrap_or_default();
    let navigator = use_navigator();
    let load_error = capabilities
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    let dirty =
        (draft() != Metadata::default() || (selection.file)().is_some()) && created().is_none();
    rsx! {
        Link { to: Route::Conventions { query: String::new() }, "Back to conventions" }
        p { "New conventions start with preorders disabled. You can choose an optional PNG or JPEG logo before creating the convention." }
        if capabilities.pending() { p { role: "status", "Loading available currencies and payment methods…" } }
        ErrorMessage { message: load_error.clone() }
        if load_error.is_some() { button { onclick: move |_| retry += 1, "Retry options" } }
        if !capabilities.pending() && load_error.is_none() && options.is_empty() { p { role: "status", "Convention creation is unavailable. No supported currencies were returned." } }
        form { novalidate: true, onsubmit: move |event| {
            event.prevent_default(); if saving() || logo_request.read().pending() || created().is_some() { return; }
            let mut input = match draft().create() { Ok(value) => value, Err(message) => { error.set(Some(message)); return; } };
            saving.set(true); error.set(None);
            let request_owner = owner();
            spawn(async move {
                let result = async {
                    if (selection.file)().is_some() {
                        let location = selection.upload(&input.slug).await?;
                        if owner() != request_owner { return Err(api::ApiError::new(reqwest::StatusCode::CONFLICT, "Session changed")); }
                        selection.staged.set(Some((input.slug.clone(), location.clone())));
                        input.logo_url = Some(location);
                    }
                    session::with_session(|| api::conventions::create(input)).await
                }.await;
                if owner() != request_owner { return; }
                saving.set(false);
                match result { Ok(record) => { selection.reset(); created.set(Some(record.id)); }, Err(value) => error.set(Some(value.to_string())) }
            });
        },
            fieldset { disabled: saving() || logo_request.read().pending() || created().is_some() || capabilities.pending() || options.is_empty(),
                MetadataFields { draft, options: options.clone(), editing: false }
                h3 { "Optional convention logo" }
                p { "PNG or JPEG only, at most 2 MiB and 4096 × 4096 pixels. If creation fails, your logo and form stay ready to retry." }
                LogoPicker { selection, request: logo_request, error, notice }
                button { r#type: "button", class: "ghost", disabled: (selection.file)().is_none(), onclick: move |_| { selection.reset(); error.set(None); }, "Discard selected file" }
                ErrorMessage { message: error() }
                button { r#type: "submit", class: "primary", if saving() { "Creating…" } else { "Create convention" } }
            }
        }
        if let Some(id) = created() {
            p { role: "status", "Convention created. Open it to manage its logo and select variants." }
            button { class: "primary", onclick: move |_| { navigator.push(Route::ConventionDetail { id: id.clone() }); }, "Open saved convention" }
        }
        DraftDepartureGuard { dirty, warning: "Your unsaved convention changes will be lost if you leave this page.".to_string() }
    }
}
#[component]
pub fn MetadataFields(
    mut draft: Signal<Metadata>,
    options: Capabilities,
    editing: bool,
) -> Element {
    let current = draft();
    let available = options
        .iter()
        .find(|(code, _)| code == &current.currency)
        .map(|(_, methods)| methods.clone())
        .unwrap_or_default();
    let unsupported_currency =
        !current.currency.is_empty() && !options.iter().any(|(code, _)| code == &current.currency);
    rsx! {
        div { class: "convention-form-grid",
            div { label { r#for: "convention-name", "Name" } input { id: "convention-name", required: true, maxlength: 120, value: current.name, oninput: move |event| draft.write().name = event.value() } }
            div { label { r#for: "convention-slug", "Slug (immutable after creation)" } input { id: "convention-slug", required: true, readonly: editing, value: current.slug, oninput: move |event| draft.write().slug = event.value() } }
            div { label { r#for: "convention-country", "Country code (ISO two-letter)" } input { id: "convention-country", required: true, maxlength: 2, value: current.country, oninput: move |event| draft.write().country = event.value().to_ascii_uppercase() } }
            div { label { r#for: "convention-currency", "Currency (immutable after creation)" } select { id: "convention-currency", required: true, disabled: editing, value: current.currency.clone(), onchange: move |event| { let mut value = draft.write(); value.currency = event.value(); value.methods.clear(); },
                option { value: "", selected: current.currency.is_empty(), "Choose currency" }
                if unsupported_currency { option { value: current.currency.clone(), selected: true, "{current.currency} (currently unsupported)" } }
                for (code, _) in &options { option { value: code.clone(), selected: code == &current.currency, "{code}" } }
            } }
            div { label { r#for: "convention-start", "Event starts on" } input { id: "convention-start", r#type: "date", required: true, value: current.start, oninput: move |event| draft.write().start = event.value() } }
            div { label { r#for: "convention-end", "Event ends on" } input { id: "convention-end", r#type: "date", required: true, value: current.end, oninput: move |event| draft.write().end = event.value() } }
        }
        fieldset { legend { "Payment methods" }
            for method in [ConventionPaymentMethod::Stripe, ConventionPaymentMethod::Paypal, ConventionPaymentMethod::Promptpay] {
                if available.contains(&method) || current.methods.contains(&method) {
                    label { class: "convention-checkbox",
                        input { r#type: "checkbox", checked: current.methods.contains(&method), disabled: !available.contains(&method) && !current.methods.contains(&method), onchange: move |event| { let mut next = draft.write(); next.methods.retain(|value| *value != method); if event.checked() { next.methods.push(method); } } }
                        "{method_label(method)}"
                        if !available.contains(&method) { " (stored method is unsupported; remove it before enabling preorders)" }
                    }
                }
            }
        }
        if unsupported_currency { p { class: "error", "The saved currency is unsupported. You can edit metadata or disable preorders. Choose a supported currency before enabling or changing payment methods." } }
        label { r#for: "convention-markdown", "Description (Markdown)" }
        textarea { id: "convention-markdown", required: true, rows: 8, value: current.markdown.clone(), oninput: move |event| draft.write().markdown = event.value() }
        section { class: "convention-preview", aria_label: "Description preview", h3 { "Description preview" } ui::MarkdownPreview { markdown: current.markdown.clone() } }
        if editing { label { class: "convention-checkbox", input { r#type: "checkbox", checked: current.enabled, onchange: move |event| draft.write().enabled = event.checked() } "Enable preorders" } }
    }
}

#[cfg(test)]
#[path = "../../tests/pages/conventions/metadata.rs"]
mod tests;
