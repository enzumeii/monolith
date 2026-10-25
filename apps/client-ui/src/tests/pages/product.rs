#![cfg(not(target_arch = "wasm32"))]

use super::product_route_id;
use crate::{
    api::tests::{catalogue, responses},
    pages::tests::{CHECKOUT, Screen, TestDocument, click, empty_cart, input, mounted, until},
};
use dioxus::prelude::*;
use domain::dto::{CatalogueImage, PublicCatalogueResponse};

fn selected_products() -> PublicCatalogueResponse {
    let mut data = catalogue();
    let shirt = &mut data.products[0];
    shirt.featured_image = Some(CatalogueImage {
        url: "https://images.test/shirt.png".into(),
    });
    shirt.variants[0].max_quantity = 2;
    shirt.variants[0].available_quantity = 2;
    let mut second = crate::tests::variant();
    second.id = "gid://shopify/ProductVariant/2".into();
    second.title = "Red / Small".into();
    second.sku = Some("SHIRT-RD-S".into());
    second.final_price = "25.75".into();
    second.max_quantity = 4;
    second.available_quantity = 4;
    second.image = Some(CatalogueImage {
        url: "https://images.test/red-shirt.png".into(),
    });
    shirt.variants.push(second);
    let mut unavailable = crate::tests::variant();
    unavailable.id = "gid://shopify/ProductVariant/4".into();
    unavailable.title = "Green / Small".into();
    unavailable.max_quantity = 0;
    unavailable.available_quantity = 0;
    shirt.variants.push(unavailable);
    let mut other = crate::tests::product();
    other.id = "gid://shopify/Product/2".into();
    other.title = "Other product".into();
    other.variants[0].id = "gid://shopify/ProductVariant/3".into();
    other.variants[0].title = "Other variant".into();
    data.products.push(other);
    data
}

fn quantity_input<'a>(html: &'a str, id: &str) -> &'a str {
    html.split("<input")
        .find(|input| input.contains(&format!("id=\"{id}\"")))
        .and_then(|input| input.split('>').next())
        .expect("quantity input attributes")
}

#[test]
fn product_route_ids_preserve_exact_positive_decimal_identity() {
    let large = "90071992547409931234567890123456789012345678901234567890";
    assert_eq!(product_route_id("gid://shopify/Product/1"), Some("1"));
    assert_eq!(
        product_route_id(&format!("gid://shopify/Product/{large}")),
        Some(large)
    );
    for invalid in [
        "",
        "1",
        "gid://shopify/Product/",
        "gid://shopify/Product/0",
        "gid://shopify/Product/01",
        "gid://shopify/Product/-1",
        "gid://shopify/Product/+1",
        "gid://shopify/Product/1.0",
        "gid://shopify/Product/1/2",
        "gid://shopify/Product/1?foo=bar",
        "gid://shopify/Product/1 ",
        "gid://shopify/Product/١",
        "gid://shopify/ProductVariant/1",
        "GID://shopify/Product/1",
    ] {
        assert_eq!(product_route_id(invalid), None, "invalid ID {invalid}");
    }
}

#[tokio::test]
async fn selected_variants_keep_prices_images_limits_and_independent_cart_quantities() {
    let (mut dom, captured) = mounted(
        Screen::Product(selected_products(), "1".into()),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    let html = dioxus_ssr::render(&dom);
    for expected in [
        "Soft cotton",
        "Clothing",
        "https://images.test/shirt.png",
        "https://images.test/red-shirt.png",
        "Blue / Large",
        "Red / Small",
        "Green / Small",
        "USD 12.30",
        "USD 25.75",
        "Currently unavailable",
        "View cart (0 items)",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
    assert!(html.contains(r#"<ul class="portal-product-variants""#));
    assert!(html.contains(r#"role="list" aria-label="Product variants""#));
    assert_eq!(html.matches("<li class=\"portal-variant\"").count(), 3);
    assert_eq!(html.matches(">Quantity</label>").count(), 3);
    assert!(!html.contains("Quantity for"));
    assert!(!html.contains("SKU"));
    assert!(!html.contains("SHIRT-BL-L"));
    assert!(!html.contains("SHIRT-RD-S"));
    assert!(!html.contains("Other product"));
    assert!(!html.contains("Other variant"));
    assert!(quantity_input(&html, "quantity-1").contains(r#"max="2""#));
    assert!(quantity_input(&html, "quantity-2").contains(r#"max="4""#));
    assert!(quantity_input(&html, "quantity-4").contains("disabled"));
    input(&mut dom, "quantity-1", "input", "invalid");
    click(&mut dom, "aria-label", "Add Shirt / Blue / Large to cart");
    assert!(dioxus_ssr::render(&dom).contains("Enter a whole-number quantity."));
    input(&mut dom, "quantity-1", "input", "3");
    click(&mut dom, "aria-label", "Add Shirt / Blue / Large to cart");
    assert!(dioxus_ssr::render(&dom).contains("Choose a quantity between 1 and 2."));
    input(&mut dom, "quantity-1", "input", "2");
    click(&mut dom, "aria-label", "Add Shirt / Blue / Large to cart");
    input(&mut dom, "quantity-2", "input", "3");
    click(&mut dom, "aria-label", "Add Shirt / Red / Small to cart");
    let html = until(&mut dom, "View cart (5 items)").await;
    assert_eq!(html.matches("Added to cart").count(), 2);
    dom.in_scope(ScopeId::ROOT, || {
        let context = captured.get().expect("mounted cart context");
        let cart = context.cart.peek();
        assert_eq!(cart.items.len(), 2);
        assert_eq!(cart.items[0].variant_id, "gid://shopify/ProductVariant/1");
        assert_eq!(cart.items[0].quantity, 2);
        assert_eq!(cart.items[0].accepted_unit_price, "12.30");
        assert_eq!(cart.items[1].variant_id, "gid://shopify/ProductVariant/2");
        assert_eq!(cart.items[1].quantity, 3);
        assert_eq!(cart.items[1].accepted_unit_price, "25.75");
    });
}

#[tokio::test]
async fn detail_matches_large_ids_exactly_and_hides_unavailable_products_and_unsafe_images() {
    let large = "90071992547409931234567890123456789012345678901234567890";
    let mut data = selected_products();
    data.products[0].id = format!("gid://shopify/Product/{large}");
    data.products[0].featured_image = Some(CatalogueImage {
        url: "javascript:unsafe-product-image".into(),
    });
    data.products[0].variants[1].image = Some(CatalogueImage {
        url: "data:text/html,unsafe-variant-image".into(),
    });
    let (dom, _) = mounted(
        Screen::Product(data.clone(), large.into()),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("Add Shirt / Red / Small to cart"));
    assert!(!html.contains("unsafe-product-image"));
    assert!(!html.contains("unsafe-variant-image"));
    for id in ["1", "9007199254740993", "0", "01", "-1", "1.0", "١", ""] {
        let (dom, _) = mounted(
            Screen::Product(data.clone(), id.into()),
            empty_cart(),
            TestDocument::new(vec![]),
        );
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains("Product unavailable"), "ID {id}: {html}");
        assert!(!html.contains("Add to cart"));
        assert!(!html.contains("quantity-"));
    }
    data.products.clear();
    let (dom, _) = mounted(
        Screen::Product(data, large.into()),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    assert!(dioxus_ssr::render(&dom).contains("Product unavailable"));
}

#[tokio::test]
async fn direct_detail_route_restores_cart_and_can_return_to_another_product_without_picker_state()
{
    let data = selected_products();
    let encoded = serde_json::to_string(&data).expect("serialize selected products");
    let mut cart = empty_cart();
    cart.add(&data.products[0].variants[0], 2)
        .expect("seed cart");
    let raw = serde_json::to_string(&cart).expect("serialize cart");
    let ((), requests) = responses(
        &[(200, &encoded), (200, &encoded), (200, &encoded)],
        async {
            let (mut dom, _) = mounted(
                Screen::Route("/fair-a/products/1".into()),
                empty_cart(),
                TestDocument::new(vec![
                    serde_json::json!({"raw":raw,"checkout_id":CHECKOUT,"blocked":false}),
                    serde_json::json!(true),
                ]),
            );
            let html = until(&mut dom, "Add Shirt / Blue / Large to cart").await;
            assert!(html.contains("View cart (2 items)"));
            let navbar = html.split("</header>").next().expect("navbar");
            assert!(!navbar.contains("Back to products"));
            assert!(
                html.split("</header>")
                    .nth(1)
                    .expect("content")
                    .contains("Back to products")
            );
            input(&mut dom, "quantity-1", "input", "invalid");
            click(&mut dom, "aria-label", "Add Shirt / Blue / Large to cart");
            assert!(dioxus_ssr::render(&dom).contains("whole-number quantity"));
            click(&mut dom, "aria-label", "Back to products");
            until(&mut dom, "View variants for Other product").await;
            click(&mut dom, "aria-label", "View variants for Other product");
            let html = until(&mut dom, "Add Other product / Other variant to cart").await;
            assert!(!html.contains("whole-number quantity"));
            assert!(!html.contains("quantity-1"));
            assert!(quantity_input(&html, "quantity-3").contains(r#"value="1""#));
            assert!(html.contains("View cart (2 items)"));
        },
    )
    .await;
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert!(request.starts_with("GET /v1/conventions/fair-a/products HTTP/1.1"));
    }
}
