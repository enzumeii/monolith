use super::{
    filters::{Filters, product_types},
    safe_image,
};
use crate::route::Route;
use dioxus::prelude::*;
use domain::dto::{PublicCatalogueResponse, PublicProduct};
use ui::{Heading, HeadingLevel};

#[component]
pub fn CataloguePage(slug: String) -> Element {
    rsx! { super::convention::ConventionPage { key: "{slug}", slug, view: super::convention::ConventionView::Catalogue } }
}

#[component]
pub fn Catalogue(catalogue: PublicCatalogueResponse) -> Element {
    let mut filters = use_signal(Filters::default);
    let matching = filters.read().matching(&catalogue.products);
    let total = matching.len();
    let current = filters.read().visible_page(total);
    let pages = filters.read().page_count(total);
    let per_page = filters.read().per_page;
    let types = product_types(&catalogue.products);
    rsx! {
        Heading { title: "Preorder collection", level: HeadingLevel::Section }
        div { class: "panel portal-filters",
            label { r#for: "product-search", "Search products" }
            input { id: "product-search", r#type: "search", value: "{filters.read().search}", placeholder: "Name or description", oninput: move |event| { let mut value = filters.write(); value.search = event.value(); value.reset(); } }
            label { r#for: "product-type", "Product type" }
            select { id: "product-type", value: "{filters.read().product_type}", onchange: move |event| { let mut value = filters.write(); value.product_type = event.value(); value.reset(); },
                option { value: "", "All types" }
                for kind in types { option { value: "{kind}", "{kind}" } }
            }
        }
        p { class: "meta", role: "status", "{total} products · Page {current} of {pages}" }
        if total == 0 { p { "No products match these filters." } }
        div { class: "portal-products",
            for product in matching.into_iter().skip((current - 1) * per_page).take(per_page) {
                ProductCard { key: "{product.id}", product: product.clone(), slug: catalogue.convention.slug.clone() }
            }
        }
        nav { class: "portal-pagination", aria_label: "Product pages",
            div { class: "portal-page-size",
                label { r#for: "page-size", "Products per page" }
                select { id: "page-size", value: "{per_page}", onchange: move |event| {
                    if let Ok(size @ (12 | 24 | 48)) = event.value().parse::<usize>() { let mut value = filters.write(); value.per_page = size; value.reset(); }
                },
                    option { value: "12", "12" } option { value: "24", "24" } option { value: "48", "48" }
                }
            }
            button { class: "ghost", disabled: current <= 1, onclick: move |_| filters.write().page = current.saturating_sub(1).max(1), "Previous" }
            span { "{current} / {pages}" }
            button { class: "ghost", disabled: current >= pages, onclick: move |_| filters.write().page = (current + 1).min(pages), "Next" }
        }
    }
}

#[component]
fn ProductCard(product: PublicProduct, slug: String) -> Element {
    let route =
        super::product::product_route_id(&product.id).map(|product_id| Route::ProductPage {
            slug,
            product_id: product_id.to_owned(),
        });
    let count = product.variants.len();
    let variant_label = if count == 1 { "variant" } else { "variants" };
    rsx! { article { class: "panel portal-product",
        if let Some(image) = product.featured_image.as_ref().filter(|image| safe_image(&image.url)) { img { class: "portal-product-image", src: "{image.url}", alt: "{product.title}", loading: "lazy" } }
        h3 { "{product.title}" }
        if !product.product_type.is_empty() { p { class: "meta", "{product.product_type}" } }
        p { class: "portal-description", "{product.description}" }
        p { class: "meta", "{count} {variant_label}" }
        if let Some(route) = route {
            Link { class: "button-link primary", to: route, aria_label: "View variants for {product.title}", "View variants" }
        }
    } }
}

#[cfg(test)]
#[path = "../tests/pages/catalogue.rs"]
mod tests;
