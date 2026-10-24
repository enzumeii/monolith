#![cfg(not(target_arch = "wasm32"))]

use crate::{
    api::tests::catalogue,
    pages::tests::{Screen, TestDocument, empty_cart, input, mounted},
};

#[tokio::test]
async fn catalogue_groups_variant_choices_behind_one_product_link_and_searches_nested_skus() {
    let mut data = catalogue();
    let mut second_variant = crate::tests::variant();
    second_variant.id = "gid://shopify/ProductVariant/2".into();
    second_variant.title = "Red / Small".into();
    second_variant.sku = Some("SHIRT-RED-S".into());
    data.products[0].variants.push(second_variant);
    data.products[0].featured_image = Some(domain::dto::CatalogueImage {
        url: "https://images.test/shirt.png".into(),
    });
    let (mut dom, _) = mounted(
        Screen::Catalogue(data),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    let html = dioxus_ssr::render(&dom);
    assert_eq!(html.matches("class=\"panel portal-product\"").count(), 1);
    assert!(html.contains("2 variants"));
    assert!(html.contains(r#"aria-label="View variants for Shirt""#));
    assert!(html.contains(r#"href="/fair-a/products/1""#));
    assert!(html.contains("https://images.test/shirt.png"));
    assert!(html.contains("Soft cotton"));
    assert!(html.contains("Clothing"));
    assert!(html.contains(r#"placeholder="Name or description""#));
    assert!(!html.contains("SKU"));
    assert!(!html.contains("SHIRT-BL-L"));
    assert!(!html.contains("SHIRT-RED-S"));
    assert!(!html.contains("Add to cart"));
    assert!(!html.contains("quantity-"));
    assert!(!html.contains("USD 12.30"));
    input(&mut dom, "product-search", "input", "shirt-red-s");
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("1 products"));
    assert_eq!(html.matches("View variants for Shirt").count(), 1);
    assert_eq!(html.matches("class=\"panel portal-product\"").count(), 1);
}

#[tokio::test]
async fn catalogue_does_not_invent_product_routes_for_invalid_product_ids() {
    let mut data = catalogue();
    data.products[0].id = "gid://shopify/ProductVariant/1".into();
    let (dom, _) = mounted(
        Screen::Catalogue(data),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("Shirt"));
    assert!(html.contains("1 variant"));
    assert!(!html.contains("/fair-a/products/"));
    assert!(!html.contains("View variants"));
    assert!(!html.contains("Add to cart"));
    assert!(!html.contains("quantity-"));
}
