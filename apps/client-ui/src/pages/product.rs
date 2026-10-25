use super::safe_image;
use crate::cart::CartContext;
use dioxus::prelude::*;
use domain::dto::{PublicCatalogueResponse, PublicProduct, PublicVariant};
use ui::{ErrorMessage, Heading, HeadingLevel};

#[component]
pub fn ProductPage(slug: String, product_id: String) -> Element {
    rsx! {
        super::convention::ConventionPage {
            key: "{slug}:{product_id}",
            slug,
            view: super::convention::ConventionView::Product { product_id },
        }
    }
}

#[component]
pub(crate) fn ProductDetail(catalogue: PublicCatalogueResponse, product_id: String) -> Element {
    let product = catalogue
        .products
        .into_iter()
        .find(|product| product_route_id(&product.id) == Some(product_id.as_str()));
    match product {
        Some(product) => rsx! {
            ProductContent {
                key: "{product.id}",
                product,
                currency: catalogue.convention.currency_code,
            }
        },
        None => rsx! {
            Heading { title: "Product unavailable", level: HeadingLevel::Section }
            p { "This product is not available for this convention." }
        },
    }
}

pub(super) fn product_route_id(id: &str) -> Option<&str> {
    let suffix = id.strip_prefix("gid://shopify/Product/")?;
    (!suffix.is_empty()
        && !suffix.starts_with('0')
        && suffix.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(suffix)
}

#[component]
fn ProductContent(product: PublicProduct, currency: String) -> Element {
    rsx! {
        article { class: "panel portal-product-detail",
            div { class: "portal-product-summary",
                if let Some(image) = product.featured_image.as_ref().filter(|image| safe_image(&image.url)) {
                    img { class: "portal-product-image", src: "{image.url}", alt: "{product.title}", loading: "lazy" }
                }
                div { class: "portal-product-summary-copy",
                    Heading { title: product.title.clone(), level: HeadingLevel::Section }
                    if !product.product_type.is_empty() { p { class: "meta", "{product.product_type}" } }
                    p { class: "portal-description", "{product.description}" }
                }
            }
            ul { class: "portal-product-variants", role: "list", aria_label: "Product variants",
                for variant in product.variants {
                    VariantPicker { key: "{variant.id}", variant, product_title: product.title.clone(), currency: currency.clone() }
                }
            }
        }
    }
}

#[component]
fn VariantPicker(variant: PublicVariant, product_title: String, currency: String) -> Element {
    let mut quantity = use_signal(|| "1".to_owned());
    let mut error = use_signal(|| None);
    let mut added = use_signal(|| false);
    let mut context = use_context::<CartContext>();
    let maximum = variant.max_quantity.min(999);
    let input_id = format!(
        "quantity-{}",
        variant.id.rsplit('/').next().unwrap_or("variant")
    );
    let add_label = format!("Add {product_title} / {} to cart", variant.title);
    let item = variant.clone();
    rsx! { li { class: "portal-variant",
        div { class: "portal-variant-media",
            if let Some(image) = variant.image.as_ref().filter(|image| safe_image(&image.url)) { img { class: "portal-variant-image", src: "{image.url}", alt: "{product_title} / {variant.title}", loading: "lazy" } }
        }
        div { class: "portal-variant-details",
            h4 { "{variant.title}" }
            p { class: "portal-price", "{currency} {variant.final_price}" }
            if maximum == 0 { p { class: "meta", "Currently unavailable" } }
        }
        div { class: "portal-variant-actions",
            label { r#for: "{input_id}", "Quantity" }
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
        }
    } }
}

#[cfg(test)]
#[path = "../tests/pages/product.rs"]
mod tests;
