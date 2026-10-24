use super::{
    filters::{Filters, product_types},
    safe_image,
};
use crate::cart::CartContext;
use dioxus::prelude::*;
use domain::dto::{PublicCatalogueResponse, PublicProduct, PublicVariant};
use ui::{ErrorMessage, Heading, HeadingLevel};

#[component]
pub fn CataloguePage(slug: String) -> Element {
    rsx! { super::convention::ConventionPage { key: "{slug}", slug, checkout_id: None } }
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
            input { id: "product-search", r#type: "search", value: "{filters.read().search}", placeholder: "Name, description or SKU", oninput: move |event| { let mut value = filters.write(); value.search = event.value(); value.reset(); } }
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
                ProductCard { key: "{product.id}", product: product.clone(), currency: catalogue.convention.currency_code.clone() }
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
fn ProductCard(product: PublicProduct, currency: String) -> Element {
    rsx! { article { class: "panel portal-product",
        if let Some(image) = product.featured_image.as_ref().filter(|image| safe_image(&image.url)) { img { class: "portal-product-image", src: "{image.url}", alt: "{product.title}", loading: "lazy" } }
        h3 { "{product.title}" }
        if !product.product_type.is_empty() { p { class: "meta", "{product.product_type}" } }
        p { class: "portal-description", "{product.description}" }
        for variant in product.variants { VariantPicker { key: "{variant.id}", variant, product_title: product.title.clone(), currency: currency.clone() } }
    } }
}

#[component]
fn VariantPicker(variant: PublicVariant, product_title: String, currency: String) -> Element {
    let mut quantity = use_signal(|| "1".to_owned());
    let mut error = use_signal(|| None);
    let mut added = use_signal(|| false);
    let mut context = use_context::<CartContext>();
    let maximum = variant.max_quantity.min(999);
    let label = format!("Quantity for {product_title} / {}", variant.title);
    let input_id = format!(
        "quantity-{}",
        variant.id.rsplit('/').next().unwrap_or("variant")
    );
    let add_label = format!("Add {product_title} / {} to cart", variant.title);
    let item = variant.clone();
    rsx! { section { class: "portal-variant",
        if let Some(image) = variant.image.as_ref().filter(|image| safe_image(&image.url)) { img { class: "portal-variant-image", src: "{image.url}", alt: "{product_title} / {variant.title}", loading: "lazy" } }
        h4 { "{variant.title}" }
        if let Some(sku) = variant.sku { p { class: "hint", "SKU: {sku}" } }
        p { class: "portal-price", "{currency} {variant.final_price}" }
        if maximum == 0 { p { class: "meta", "Currently unavailable" } }
        label { r#for: "{input_id}", "{label}" }
        div { class: "portal-quantity",
            input { id: "{input_id}", r#type: "number", min: "1", max: "{maximum}", step: "1", value: "{quantity}", disabled: maximum == 0,
                oninput: move |event| { quantity.set(event.value()); added.set(false); } }
            button { class: "primary", disabled: maximum == 0, aria_label: add_label, onclick: move |_| {
                let result = quantity.read().parse::<u32>().map_err(|_| "Enter a whole-number quantity.".to_owned()).and_then(|quantity| context.update(|cart| cart.add(&item, quantity)));
                added.set(result.is_ok()); error.set(result.err());
            }, "Add to cart" }
        }
        ErrorMessage { message: error.read().clone() }
        if *added.read() { p { class: "hint", role: "status", "Added to cart" } }
    } }
}
