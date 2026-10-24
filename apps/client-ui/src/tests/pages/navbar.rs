#![cfg(not(target_arch = "wasm32"))]

use crate::{
    api::tests::{catalogue, convention, responses},
    pages::tests::{CHECKOUT, Screen, TestDocument, empty_cart, mounted, until},
};

#[tokio::test]
async fn restored_catalogue_cart_badge_links_to_checkout_without_variant_controls() {
    let (mut dom, _) = mounted(
        Screen::Provider(catalogue()),
        empty_cart(),
        TestDocument::new(vec![
            serde_json::json!({"raw":null,"checkout_id":CHECKOUT,"blocked":false}),
            serde_json::json!(true),
            serde_json::json!(true),
        ]),
    );
    let restoring = dioxus_ssr::render(&dom);
    assert!(!restoring.contains("All conventions"));
    assert!(!restoring.contains("portal-cart-link"));
    assert!(!restoring.contains("Back to products"));
    let html = until(&mut dom, "View cart (0 items)").await;
    assert!(!html.contains("All conventions"));
    assert!(html.contains(r#"alt="Enzumeii""#));
    let navbar = html.split("</header>").next().expect("navbar markup");
    assert!(navbar.contains(&format!(r#"href="/fair-a/cart/{CHECKOUT}""#)));
    assert!(navbar.contains("portal-cart-count"));
    assert!(navbar.contains(">0</span>"));
    assert!(!html.contains("Add to cart"));
}

#[tokio::test]
async fn cart_navbar_returns_to_products_with_restored_quantity_count() {
    let mut cart = empty_cart();
    cart.add(&crate::tests::variant(), 3).expect("seed cart");
    let raw = serde_json::to_string(&cart).expect("serialize cart");
    let data = serde_json::to_string(&catalogue()).expect("serialize catalogue");
    responses(&[(200, &data)], async {
        let (mut dom, _) = mounted(
            Screen::Route("/fair-a/cart/unknown".into()),
            empty_cart(),
            TestDocument::new(vec![
                serde_json::json!({"raw":raw,"checkout_id":CHECKOUT,"blocked":false}),
                serde_json::json!(true),
            ]),
        );
        let html = until(&mut dom, "Browse products (3 items in cart)").await;
        let navbar = html.split("</header>").next().expect("navbar markup");
        assert!(navbar.contains(r#"href="/fair-a""#));
        assert!(navbar.contains("portal-cart-icon"));
        assert!(!navbar.contains("Back to products"));
        let content = html.split("</header>").nth(1).expect("page content");
        assert!(content.contains("Back to products"));
        assert!(
            content.find("portal-back-link").expect("back control")
                < content.find("content-heading").expect("convention heading")
        );
        assert!(navbar.contains(">3</span>"));
        assert!(!html.contains("All conventions"));
        assert!(html.contains("Cart unavailable"));
    })
    .await;
}

#[tokio::test]
async fn directory_shows_brand_and_product_links_without_redundant_navigation() {
    let list = serde_json::json!({
        "items":[convention("a"), convention("b")],
        "page":1,"per_page":100,"has_next":false,"visitor_country":"TH"
    })
    .to_string();
    responses(&[(200, &list)], async {
        let (mut dom, _) = mounted(
            Screen::Route("/".into()),
            empty_cart(),
            TestDocument::new(vec![serde_json::json!(["2099-01-01", "TH"])]),
        );
        let html = until(&mut dom, "Browse products").await;
        assert!(html.contains(r#"alt="Enzumeii""#));
        assert!(html.contains("Enzumeii home"));
        assert_eq!(html.matches("Browse products").count(), 2);
        assert!(!html.contains("All conventions"));
        assert!(!html.contains("portal-cart-link"));
    })
    .await;
}
