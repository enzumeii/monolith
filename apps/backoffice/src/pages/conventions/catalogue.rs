use super::state::Selection;
use crate::{
    api,
    components::{
        ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    session,
};
use dioxus::prelude::*;
use domain::dto::{CatalogueQuery, CatalogueResponse};

#[derive(Clone, Default, PartialEq)]
struct BrowseQuery {
    location: String,
    search: String,
    cursor: Option<String>,
}
impl BrowseQuery {
    fn to_api_query(&self) -> CatalogueQuery {
        CatalogueQuery {
            location_id: format!("gid://shopify/Location/{}", self.location),
            page_size: Some(25),
            cursor: self.cursor.clone(),
            search: (!self.search.is_empty()).then(|| self.search.clone()),
        }
    }
}
#[derive(Clone, PartialEq)]
struct SourceVariant {
    product_id: String,
    product_title: String,
    description: String,
    product_status: String,
    id: String,
    title: String,
    image: Option<(String, String)>,
    options: String,
    price: String,
    currency: String,
    available: Option<i64>,
    eligible: bool,
    reason: String,
}
#[derive(Clone, PartialEq)]
struct BrowsePage {
    items: Vec<SourceVariant>,
    has_next: bool,
    cursor: Option<String>,
}
impl BrowsePage {
    fn from_response(response: CatalogueResponse) -> Self {
        let mut items = Vec::new();
        for product in response.products {
            for variant in product.variants {
                let image = variant
                    .image
                    .as_ref()
                    .or(product.featured_image.as_ref())
                    .filter(|image| safe_image_url(&image.url))
                    .map(|image| {
                        (
                            image.url.clone(),
                            image
                                .alt_text
                                .clone()
                                .unwrap_or_else(|| product.title.clone()),
                        )
                    });
                items.push(SourceVariant {
                    product_id: product.id.clone(),
                    product_title: product.title.clone(),
                    description: product.description.clone(),
                    product_status: product.status.clone(),
                    id: variant.id,
                    title: variant.title,
                    image,
                    options: variant
                        .selected_options
                        .iter()
                        .map(|option| format!("{}: {}", option.name, option.value))
                        .collect::<Vec<_>>()
                        .join(" · "),
                    price: variant.price.amount,
                    currency: variant.price.currency_code,
                    available: variant.available_quantity,
                    eligible: variant.eligible,
                    reason: variant
                        .unavailable_reason
                        .map_or_else(String::new, |reason| format!("{reason:?}")),
                });
            }
        }
        Self {
            items,
            has_next: response.page_info.has_next_page,
            cursor: response.page_info.end_cursor,
        }
    }
}
#[component]
pub fn SelectionEditor(
    id: String,
    currency: String,
    enabled: bool,
    on_dirty: EventHandler<bool>,
) -> Element {
    let owner = use_owner();
    let mut selection = use_signal(Selection::default);
    let mut saved = use_signal(Selection::default);
    let mut retry = use_signal(|| 0_u64);
    let load_id = id.clone();
    let memberships = use_resource(move || {
        let _ = retry();
        let id = load_id.clone();
        async move { session::with_session(|| api::conventions::all_variants(&id)).await }
    });
    use_effect(move || {
        if let Some(result) = memberships.read().as_ref() {
            match result {
                Ok(items) => {
                    let value = Selection::from_saved(items);
                    selection.set(value.clone());
                    saved.set(value);
                }
                Err(_) => {
                    selection.write().loaded = false;
                }
            }
        }
    });
    let mut saving = use_signal(RequestState::default);
    let mut save_error = use_signal(|| None::<String>);
    let mut save_notice = use_signal(|| None::<String>);
    let mut location = use_signal(String::new);
    let mut search = use_signal(String::new);
    let mut browse_error = use_signal(|| None::<String>);
    let mut submitted = use_signal(|| None::<BrowseQuery>);
    let browse = use_resource(move || {
        let query = submitted();
        async move {
            if let Some(query) = query {
                let result =
                    session::with_session(|| api::catalogue::products(query.to_api_query()))
                        .await
                        .map(BrowsePage::from_response);
                Some((query, result))
            } else {
                None
            }
        }
    });
    use_effect(move || on_dirty(selection() != saved() || saving.read().pending()));
    let load_error = memberships
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    let page = if browse.pending() {
        None
    } else {
        browse
            .read()
            .as_ref()
            .and_then(Option::as_ref)
            .filter(|(query, _)| Some(query) == submitted.read().as_ref())
            .and_then(|(_, result)| result.as_ref().ok())
            .cloned()
    };
    let upstream_error = browse
        .read()
        .as_ref()
        .and_then(Option::as_ref)
        .filter(|(query, _)| Some(query) == submitted.read().as_ref())
        .and_then(|(_, result)| result.as_ref().err())
        .map(ToString::to_string);
    let current = selection();
    let rows: Vec<_> = current.items.values().cloned().collect();
    let save_id = id;
    let next_cursor = page
        .as_ref()
        .filter(|page| page.has_next)
        .and_then(|page| page.cursor.clone());
    rsx! {
        section { class: "panel convention-section", aria_label: "Selected variants and prices",
            h3 { "Selected variants and final prices" }
            p { "Memberships and final prices save together. Removing a row removes its membership; there is no per-variant enabled flag. Up to 250 variants." }
            if memberships.pending() || !current.loaded { p { role: "status", "All saved membership pages must load before editing or saving." } }
            ErrorMessage { message: load_error.clone() }
            if load_error.is_some() { button { onclick: move |_| { selection.write().loaded = false; retry += 1; }, "Retry all saved memberships" } }
            p { "Selected: {rows.len()} / 250 · Convention currency: {currency}" }
            div { class: "convention-table-scroll",
                table { class: "convention-table",
                    caption { "Selected memberships and convention prices" }
                    thead { tr { th { "Variant" } th { "Current source details" } th { "Final convention price" } th { "Action" } } }
                    tbody { for row in rows { tr { key: "{row.shopify_variant_id}",
                        td { "Variant {row.shopify_variant_id}" p { "Product {row.shopify_product_id}" } }
                        td {
                            if let Some(source) = page.as_ref().and_then(|page| page.items.iter().find(|source| source.id == row.shopify_variant_id)) {
                                strong { "{source.product_title} · {source.title}" }
                                p { "Source price: {source.price} {source.currency}" }
                                p { "Available stock: {stock_label(source.available)}" }
                                p { if source.eligible { "Eligible" } else { "Unavailable: {source.reason}" } }
                            } else { "Source details not loaded" }
                        }
                        td { label { r#for: "variant-price-{row.shopify_variant_id}", "Final price ({currency})" }
                            input { id: "variant-price-{row.shopify_variant_id}", r#type: "text", inputmode: "decimal", value: row.final_price.clone(), disabled: !current.loaded || saving.read().pending(),
                                oninput: { let variant_id = row.shopify_variant_id.clone(); move |event| { if let Some(item) = selection.write().items.get_mut(&variant_id) { item.final_price = event.value(); } save_notice.set(None); } }
                            }
                        }
                        td { button { r#type: "button", class: "ghost", disabled: !current.loaded || saving.read().pending(), onclick: { let variant_id = row.shopify_variant_id.clone(); move |_| { selection.write().items.remove(&variant_id); save_notice.set(None); } }, "Remove" } }
                    } } }
                }
            }
            if enabled && current.items.is_empty() { p { class: "error", "Disable preorders and save metadata before clearing all selected variants." } }
            ErrorMessage { message: save_error() }
            if let Some(message) = save_notice() { p { role: "status", "{message}" } }
            button { r#type: "button", class: "primary", disabled: saving.read().pending() || memberships.pending() || !current.loaded || current == saved() || (enabled && current.items.is_empty()),
                onclick: move |_| {
                    let snapshot = selection(); let input = match snapshot.request() { Ok(value) => value, Err(error) => { save_error.set(Some(error)); return; } };
                    let Some(ticket) = saving.write().begin() else { return; }; let id = save_id.clone(); let request_owner = owner(); save_error.set(None); save_notice.set(None);
                    spawn(async move { let result = session::with_session(|| api::conventions::replace_variants(&id, input)).await;
                        if owner() != request_owner || !saving.write().finish(ticket) { return; }
                        match result { Ok(value) => { let saved_value = Selection::from_saved(&value.items); saved.set(saved_value.clone()); if selection() == snapshot { selection.set(saved_value); } save_notice.set(Some("Selected variants and prices saved".into())); }, Err(error) => save_error.set(Some(error.to_string())) }
                    });
                }, if saving.read().pending() { "Saving variants…" } else { "Save selected variants and prices" }
            }
        }
        section { class: "panel convention-section", aria_label: "Shopify catalogue",
            h3 { "Browse Shopify catalogue" }
            p { "Source prices and location-specific available stock are informational at the time this page loads. They are separate from final convention prices and are revalidated by the backend when saving." }
            form { onsubmit: move |event| { event.prevent_default(); if !api::valid_location_id(&location()) { browse_error.set(Some("Enter a positive numeric location ID".into())); return; } browse_error.set(None); submitted.set(Some(BrowseQuery { location: location(), search: search(), cursor: None })); },
                div { class: "convention-form-grid",
                    div { label { r#for: "catalogue-location", "Shopify location ID" } input { id: "catalogue-location", r#type: "text", inputmode: "numeric", required: true, value: location(), oninput: move |event| location.set(event.value()) } }
                    div { label { r#for: "catalogue-search", "Shopify search (forwarded unchanged)" } input { id: "catalogue-search", value: search(), oninput: move |event| search.set(event.value()) } }
                }
                button { r#type: "submit", class: "primary", disabled: browse.pending(), "Search catalogue" }
            }
            ErrorMessage { message: browse_error().or(upstream_error) }
            if browse.pending() && submitted().is_some() { p { role: "status", "Loading current catalogue page…" } }
            if let Some(page) = page {
                if page.items.is_empty() { p { "No variants match this search at this location." } }
                div { class: "convention-catalogue-grid",
                    for source in page.items { article { class: "convention-product", key: "{source.id}",
                        if let Some((url, alt)) = source.image { img { src: url, alt, loading: "lazy" } }
                        h4 { "{source.product_title} · {source.title}" }
                        p { "{source.description}" } p { "{source.options}" } p { "Product status: {source.product_status}" }
                        p { "Source price: {source.price} {source.currency}" } p { "Available stock: {stock_label(source.available)}" }
                        p { if source.eligible { "Eligible" } else { "Unavailable: {source.reason}" } }
                        button { r#type: "button", class: "ghost", disabled: !current.loaded || memberships.pending() || saving.read().pending() || !source.eligible || current.items.contains_key(&source.id) || current.items.len() >= 250,
                            onclick: move |_| { selection.write().insert(source.product_id.clone(), source.id.clone()); save_notice.set(None); },
                            if current.items.contains_key(&source.id) { "Already selected" } else { "Select variant" }
                        }
                    } }
                }
            }
            if let Some(cursor) = next_cursor {
                button { r#type: "button", class: "ghost", disabled: browse.pending(), onclick: move |_| { if let Some(mut query) = submitted() { query.cursor = Some(cursor.clone()); submitted.set(Some(query)); } }, "Next catalogue page" }
            }
        }
    }
}
fn stock_label(available: Option<i64>) -> String {
    available.map_or_else(
        || "Missing or inactive inventory level".into(),
        |value| value.to_string(),
    )
}
fn safe_image_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
#[path = "../../tests/pages/conventions/catalogue.rs"]
mod tests;
