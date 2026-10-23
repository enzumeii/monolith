use super::state::{Metadata, method_label};
use crate::{
    api,
    components::{DraftDepartureGuard, ErrorMessage},
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::ConventionPaymentMethod;

pub type Capabilities = Vec<(String, Vec<ConventionPaymentMethod>)>;

#[component]
pub fn CreateEditor() -> Element {
    let draft = use_signal(Metadata::default);
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
    let dirty = draft() != Metadata::default() && created().is_none();
    rsx! {
        Link { to: Route::Conventions { query: String::new() }, "Back to conventions" }
        p { "New conventions start with preorders disabled. Save the convention before uploading its logo." }
        if capabilities.pending() { p { role: "status", "Loading available currencies and payment methods…" } }
        ErrorMessage { message: load_error.clone() }
        if load_error.is_some() { button { onclick: move |_| retry += 1, "Retry options" } }
        if !capabilities.pending() && load_error.is_none() && options.is_empty() { p { role: "status", "Convention creation is unavailable. Configure backend currency and payment capabilities first." } }
        form { novalidate: true, onsubmit: move |event| {
            event.prevent_default(); if saving() || created().is_some() { return; }
            let input = match draft().create() { Ok(value) => value, Err(message) => { error.set(Some(message)); return; } };
            saving.set(true); error.set(None);
            spawn(async move { let result = session::with_session(|| api::conventions::create(input)).await; saving.set(false); match result { Ok(record) => { created.set(Some(record.id)); }, Err(value) => error.set(Some(value.to_string())) } });
        },
            fieldset { disabled: saving() || created().is_some() || capabilities.pending() || options.is_empty(),
                MetadataFields { draft, options: options.clone(), editing: false }
                ErrorMessage { message: error() }
                button { r#type: "submit", class: "primary", if saving() { "Creating…" } else { "Create convention" } }
            }
        }
        if let Some(id) = created() {
            p { role: "status", "Convention created. Open it to upload a logo and select variants." }
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
                        if !available.contains(&method) { " (stored method no longer configured; remove it or restore its backend capability)" }
                    }
                }
            }
        }
        if unsupported_currency { p { class: "error", "The saved currency is no longer configured. You can edit metadata or disable preorders. Restore capabilities before enabling or changing payment methods." } }
        label { r#for: "convention-markdown", "Description (Markdown)" }
        textarea { id: "convention-markdown", required: true, rows: 8, value: current.markdown.clone(), oninput: move |event| draft.write().markdown = event.value() }
        section { class: "convention-preview", aria_label: "Description preview", h3 { "Description preview" } ui::MarkdownPreview { markdown: current.markdown.clone() } }
        if editing { label { class: "convention-checkbox", input { r#type: "checkbox", checked: current.enabled, onchange: move |event| draft.write().enabled = event.checked() } "Enable preorders" } }
    }
}

#[cfg(test)]
#[path = "../../tests/pages/conventions/metadata.rs"]
mod tests;
