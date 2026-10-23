use super::state::{Selection, VariantDetails, merge_saved_details};
use crate::{
    api,
    components::{
        ErrorMessage, Popup,
        request_state::{RequestState, use_owner},
    },
    session,
};
use dioxus::prelude::*;
use domain::dto::{CatalogueQuery, CatalogueResponse};
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq)]
struct BrowseQuery {
    location: String,
    search: String,
    cursor: Option<String>,
}
impl BrowseQuery {
    fn to_api_query(&self) -> CatalogueQuery {
        CatalogueQuery {
            location_id: self.location.clone(),
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
    id: String,
    title: String,
    sku: Option<String>,
    image: Option<(String, String)>,
    product_image: Option<(String, String)>,
    price: String,
    currency: String,
    available: Option<i64>,
    eligible: bool,
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
                    .filter(|image| safe_image_url(&image.url))
                    .map(|image| {
                        (
                            image.url.clone(),
                            detail_text(Some(&variant.title), "Variant image").to_owned(),
                        )
                    });
                items.push(SourceVariant {
                    product_image: product
                        .featured_image
                        .as_ref()
                        .filter(|image| safe_image_url(&image.url))
                        .map(|image| {
                            (
                                image.url.clone(),
                                format!(
                                    "Product: {}",
                                    detail_text(Some(&product.title), "Product image")
                                ),
                            )
                        }),
                    product_id: product.id.clone(),
                    product_title: product.title.clone(),
                    id: variant.id,
                    title: variant.title,
                    sku: variant.sku,
                    image,
                    price: variant.price.amount,
                    currency: variant.price.currency_code,
                    available: variant.available_quantity,
                    eligible: variant.eligible,
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
    let mut details = use_signal(BTreeMap::<String, VariantDetails>::new);
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
                    let value = Selection::from_saved(items.iter().map(|item| &item.membership));
                    merge_saved_details(&mut details.write(), items);
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
    let settings = use_context::<crate::config::Settings>();
    let location = settings.conventions.catalogue_location_id;
    let mut product_type = use_signal(String::new);
    let mut title = use_signal(String::new);
    let mut picker_open = use_signal(|| false);
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
    let rows: Vec<_> = current
        .items
        .values()
        .map(|row| {
            (
                row.clone(),
                details
                    .read()
                    .get(&row.shopify_variant_id)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect();
    let save_id = id;
    let next_cursor = page
        .as_ref()
        .filter(|page| page.has_next)
        .and_then(|page| page.cursor.clone());
    rsx! {
        section { class: "panel convention-section", aria_label: "Selected variants and prices",
            header { class: "convention-pane-heading",
                h3 { "Selected variants and final prices" }
                button { id: "browse-shopify-catalogue", r#type: "button", class: "primary", onclick: move |_| picker_open.set(true), "Browse Shopify catalogue" }
            }
            if memberships.pending() || !current.loaded { p { role: "status", "All saved membership pages must load before editing or saving." } }
            ErrorMessage { message: load_error.clone() }
            if load_error.is_some() { button { r#type: "button", class: "ghost", onclick: move |_| { selection.write().loaded = false; retry += 1; }, "Retry all saved memberships" } }
            div { class: "convention-table-scroll",
                table { class: "convention-table",
                    caption { class: "visually-hidden", "Selected variants and final prices" }
                    thead { tr { th { "Variant" } th { "Variant image and stock" } th { "Final convention price" } th { "Action" } } }
                    tbody { for (row, detail) in rows { tr { key: "{row.shopify_variant_id}",
                        td {
                            VariantTitle { sku: detail.sku.clone(), product: detail.product_title.clone(), variant: detail.variant_title.clone() }
                        }
                        td {
                            if let Some((url, alt)) = detail.variant_image.filter(|(url, _)| safe_image_url(url)) {
                                img { class: "convention-variant-image", src: url, alt, loading: "lazy" }
                            } else { p { "No variant image" } }
                            p { "Available stock: {selected_stock_label(detail.available)}" }
                            if let Some(location) = detail.stock_location {
                                p { "Last loaded at location {location}" }
                            }
                        }
                        td { label { class: "convention-label", r#for: "variant-price-{row.shopify_variant_id}", "Final price ({currency})" }
                            input { id: "variant-price-{row.shopify_variant_id}", r#type: "text", inputmode: "decimal", value: row.final_price.clone(), disabled: !current.loaded || saving.read().pending(),
                                oninput: { let variant_id = row.shopify_variant_id.clone(); move |event| { if let Some(item) = selection.write().items.get_mut(&variant_id) { item.final_price = event.value(); } save_notice.set(None); } }
                            }
                        }
                        td { button { r#type: "button", class: "ghost", disabled: !current.loaded || saving.read().pending(), onclick: { let variant_id = row.shopify_variant_id.clone(); move |_| { selection.write().items.remove(&variant_id); details.write().remove(&variant_id); save_notice.set(None); } }, "Remove" } }
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
                        match result { Ok(value) => { let saved_value = Selection::from_saved(value.items.iter().map(|item| &item.membership)); merge_saved_details(&mut details.write(), &value.items); saved.set(saved_value.clone()); if selection() == snapshot { selection.set(saved_value); } save_notice.set(Some("Selected variants and prices saved".into())); }, Err(error) => save_error.set(Some(error.to_string())) }
                    });
                }, if saving.read().pending() { "Saving variants…" } else { "Save selected variants and prices" }
            }
        }
        Popup { open: picker_open(), title: "Browse Shopify catalogue", class: "convention-catalogue-popup", on_close: move |()| picker_open.set(false),
            section { class: "convention-section", aria_label: "Shopify catalogue",
                p { "Source prices and available stock are informational when loaded. Final convention prices are edited outside this catalogue and revalidated when saving." }
                form { onsubmit: move |event| {
                    event.prevent_default();
                    if browse.pending() { return; }
                    let search = match catalogue_search(&product_type(), &title()) {
                        Ok(search) => search,
                        Err(error) => { browse_error.set(Some(error)); return; }
                    };
                    browse_error.set(None);
                    submitted.set(Some(BrowseQuery { location: location.clone(), search, cursor: None }));
                },
                    div { class: "convention-form-grid",
                        div { label { class: "convention-label", r#for: "catalogue-product-type", "Product type" }
                            input { id: "catalogue-product-type", value: product_type(), oninput: move |event| product_type.set(event.value()) }
                        }
                        div { label { class: "convention-label", r#for: "catalogue-title", "Product title" }
                            input { id: "catalogue-title", value: title(), oninput: move |event| title.set(event.value()) }
                        }
                    }
                    button { r#type: "submit", class: "primary", disabled: browse.pending(), "Search catalogue" }
                }
                ErrorMessage { message: browse_error().or(upstream_error) }
                if browse.pending() && submitted().is_some() { p { role: "status", "Loading current catalogue page…" } }
                if let Some(page) = page {
                    if page.items.is_empty() { p { "No variants match this search at this location." } }
                    div { class: "convention-table-scroll",
                        table { class: "convention-table",
                            caption { "Shopify products and variants" }
                            thead { tr { th { "Images" } th { "Product and variant" } th { "Source price and stock" } th { "Action" } } }
                            tbody { for source in page.items { tr { key: "{source.id}",
                                td { div { class: "convention-result-images",
                                    CatalogueImage { image: source.product_image.clone(), label: "Product", fallback: "No product image" }
                                    CatalogueImage { image: source.image.clone(), label: "Variant", fallback: "No variant image" }
                                } }
                                td {
                                    VariantTitle { sku: source.sku.clone(), product: Some(source.product_title.clone()), variant: Some(source.title.clone()) }
                                }
                                td {
                                    span { class: "convention-label", "Source price" }
                                    p { class: "convention-value", "{source.price} {source.currency}" }
                                    span { class: "convention-label", "Available stock" }
                                    p { class: "convention-value", "{stock_label(source.available)}" }
                                    p { class: if source.eligible { "convention-status is-enabled" } else { "convention-status is-disabled" }, if source.eligible { "Eligible" } else { "Unavailable" } }
                                }
                                td { button { r#type: "button", class: "primary", disabled: !current.loaded || memberships.pending() || saving.read().pending() || !source.eligible || current.items.contains_key(&source.id) || current.items.len() >= 250,
                                    onclick: move |_| {
                                        if !selection.read().loaded || saving.read().pending() || !source.eligible || selection.read().items.contains_key(&source.id) || selection.read().items.len() >= 250 { return; }
                                        details.write().insert(source.id.clone(), VariantDetails {
                                            product_title: Some(source.product_title.clone()), variant_title: Some(source.title.clone()), sku: source.sku.clone(), variant_image: source.image.clone(), available: source.available,
                                            stock_location: submitted().map(|query| query.location),
                                        }); selection.write().insert(source.product_id.clone(), source.id.clone()); save_notice.set(None);
                                    },
                                    if current.items.contains_key(&source.id) { "Already selected" } else { "Select variant" }
                                } }
                            } } }
                        }
                    }
                }
                if let Some(cursor) = next_cursor {
                    button { r#type: "button", class: "ghost", disabled: browse.pending(), onclick: move |_| { if let Some(mut query) = submitted() { query.cursor = Some(cursor.clone()); submitted.set(Some(query)); } }, "Next catalogue page" }
                }
            }
        }
    }
}
#[component]
fn VariantTitle(sku: Option<String>, product: Option<String>, variant: Option<String>) -> Element {
    rsx! {
        p { class: "convention-variant-title",
            strong { "[" {detail_text(sku.as_deref(), "SKU unavailable")} "] " {detail_text(product.as_deref(), "Product title unavailable")} " - " }
            span { {detail_text(variant.as_deref(), "Variant title unavailable")} }
        }
    }
}
#[component]
fn CatalogueImage(image: Option<(String, String)>, label: String, fallback: String) -> Element {
    let mut failed = use_signal(|| false);
    let reset_image = image.clone();
    use_effect(use_reactive!(|reset_image| {
        let _ = reset_image;
        failed.set(false);
    }));
    rsx! {
        figure { class: "convention-result-image",
            figcaption { class: "convention-label", "{label}" }
            if let Some((url, alt)) = image.filter(|(url, _)| safe_image_url(url) && !failed()) {
                img { class: "convention-thumbnail", src: url, alt, loading: "lazy", width: "160", height: "160", onerror: move |_| failed.set(true) }
            } else { span { class: "convention-label", "{fallback}" } }
        }
    }
}

fn catalogue_search(product_type: &str, title: &str) -> Result<String, String> {
    let clauses = [("product_type", product_type), ("title", title)]
        .into_iter()
        .filter_map(|(field, value)| {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            let mut escaped = String::new();
            for character in value.chars() {
                if matches!(character, '\\' | '"' | ':' | '(' | ')') {
                    escaped.push('\\');
                }
                escaped.push(character);
            }
            Some(format!("{field}:\"{escaped}\""))
        })
        .collect::<Vec<_>>()
        .join(" AND ");
    if clauses.len() > 2048 {
        return Err("Search is too long; shorten the product type or title (maximum 2048 bytes after escaping).".into());
    }
    Ok(clauses)
}

fn detail_text<'a>(value: Option<&'a str>, unavailable: &'a str) -> &'a str {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(unavailable)
}
fn selected_stock_label(available: Option<i64>) -> String {
    available.map_or_else(|| "Not available".into(), |value| value.to_string())
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
