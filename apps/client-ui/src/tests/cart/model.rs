use super::*;
use crate::tests::{product, variant};

const CHECKOUT: &str = "f79b7e64-8e36-4aca-9e21-ecb7cb513700";

fn cart() -> Cart {
    Cart::new("convention-a".into(), CHECKOUT.into())
}

#[test]
fn restore_requires_matching_convention_supported_version_unique_valid_lines_and_checkout() {
    let mut cart = cart();
    cart.add(&variant(), 2).expect("add");
    let raw = serde_json::to_string(&cart).expect("json");
    assert_eq!(Cart::restore(&raw, "convention-a"), Some(cart.clone()));
    assert!(Cart::restore(&raw, "convention-b").is_none());
    for invalid in [
        raw.replace("\"version\":1", "\"version\":2"),
        raw.replace(CHECKOUT, "arbitrary"),
        raw.replace("\"quantity\":2", "\"quantity\":0"),
        "corrupt".into(),
    ] {
        assert!(Cart::restore(&invalid, "convention-a").is_none());
    }
    cart.items.push(cart.items[0].clone());
    assert!(Cart::restore(&serde_json::to_string(&cart).expect("json"), "convention-a").is_none());
}

#[test]
fn add_merges_without_exceeding_stock_and_does_not_accept_changed_price_implicitly() {
    let mut cart = cart();
    let mut variant = variant();
    cart.add(&variant, 2).expect("add");
    variant.final_price = "15.00".into();
    cart.add(&variant, 3).expect("merge");
    assert_eq!(cart.items[0].quantity, 5);
    assert_eq!(cart.items[0].accepted_unit_price, "12.30");
    assert!(cart.add(&variant, 6).is_err());
    assert_eq!(cart.items[0].quantity, 5);
    assert!(cart.add(&variant, 0).is_err());
}

#[test]
fn reconciliation_preserves_missing_selections_and_requires_explicit_price_acceptance() {
    let mut cart = cart();
    cart.add(&variant(), 5).expect("add");
    let mut product = product();
    product.variants[0].max_quantity = 2;
    product.variants[0].final_price = "99.00".into();
    let lines = reconciliation(&cart, &[product.clone()]);
    assert!(lines[0].price_changed);
    assert!(lines[0].issue.is_some());
    assert_eq!(cart.items[0].quantity, 5);
    cart.set_quantity(&variant().id, 2);
    cart.accept_prices(&[product.clone()]);
    let lines = reconciliation(&cart, &[product]);
    assert!(!lines[0].price_changed);
    assert!(lines[0].issue.is_none());
    assert!(reconciliation(&cart, &[])[0].issue.is_some());
    cart.remove(&variant().id);
    assert_eq!(cart.quote_request().items, []);
}

#[test]
fn authoritative_quote_price_race_requires_acknowledgement() {
    let mut cart = cart();
    cart.add(&variant(), 2).expect("add");
    let mut catalogue = catalogue();
    assert!(!reconciliation(&cart, &catalogue.products)[0].price_changed);
    let quote = domain::dto::ConventionQuote {
        currency_code: "USD".into(),
        items: vec![domain::dto::ConventionQuoteLine {
            variant_id: variant().id,
            quantity: 2,
            unit_price: "120.00".into(),
            line_total: "240.00".into(),
        }],
        subtotal: "240.00".into(),
        shipping: "0.00".into(),
        total: "240.00".into(),
    };
    assert!(apply_quote_prices(&cart, "USD", &mut catalogue, &quote));
    assert!(reconciliation(&cart, &catalogue.products)[0].price_changed);
    assert_eq!(cart.items[0].accepted_unit_price, "12.30");
    cart.accept_prices(&catalogue.products);
    assert!(!reconciliation(&cart, &catalogue.products)[0].price_changed);
}

fn catalogue() -> domain::dto::PublicCatalogueResponse {
    domain::dto::PublicCatalogueResponse {
        convention: domain::dto::PublicConvention {
            id: "convention-a".into(),
            slug: "shared-slug".into(),
            name: "Original event".into(),
            logo_preview_url: None,
            logo_preview_url_expires_at: None,
            country_code: "TH".into(),
            event_starts_on: "2026-11-01".into(),
            event_ends_on: "2026-11-02".into(),
            description_markdown: String::new(),
            currency_code: "USD".into(),
        },
        products: vec![product()],
    }
}

#[test]
fn replacement_convention_or_currency_cannot_supply_an_old_carts_quote() {
    let mut cart = cart();
    cart.add(&variant(), 1).expect("add");
    let original_cart = cart.clone();
    let quote = domain::dto::ConventionQuote {
        currency_code: "USD".into(),
        items: vec![domain::dto::ConventionQuoteLine {
            variant_id: variant().id,
            quantity: 1,
            unit_price: "12.30".into(),
            line_total: "12.30".into(),
        }],
        subtotal: "12.30".into(),
        shipping: "0.00".into(),
        total: "12.30".into(),
    };
    for (id, catalogue_currency, quote_currency) in [
        ("convention-b", "USD", "USD"),
        ("convention-b", "SGD", "SGD"),
        ("convention-a", "SGD", "SGD"),
        ("convention-a", "USD", "SGD"),
    ] {
        let mut replacement = catalogue();
        replacement.convention.id = id.into();
        replacement.convention.currency_code = catalogue_currency.into();
        let mut replacement_quote = quote.clone();
        replacement_quote.currency_code = quote_currency.into();
        let untouched = replacement.clone();
        assert!(
            !apply_quote_prices(&cart, "USD", &mut replacement, &replacement_quote),
            "identity/currency {id}/{catalogue_currency}/{quote_currency}"
        );
        assert_eq!(replacement, untouched);
        assert_eq!(cart, original_cart);
    }
}

#[test]
fn cart_images_use_current_variant_then_product_and_are_not_persisted() {
    let mut cart = cart();
    cart.add(&variant(), 1).expect("add");
    let mut product = product();
    let product_image = domain::dto::CatalogueImage {
        url: "https://images.test/product.png".into(),
    };
    let variant_image = domain::dto::CatalogueImage {
        url: "https://images.test/variant.png".into(),
    };
    product.featured_image = Some(product_image.clone());
    product.variants[0].image = Some(variant_image.clone());
    assert_eq!(
        reconciliation(&cart, &[product.clone()])[0].image,
        Some(variant_image)
    );
    product.variants[0].image = None;
    assert_eq!(
        reconciliation(&cart, &[product.clone()])[0].image,
        Some(product_image)
    );
    product.featured_image = None;
    assert!(reconciliation(&cart, &[product])[0].image.is_none());
    assert!(reconciliation(&cart, &[])[0].image.is_none());
    let stored = serde_json::to_string(&cart).expect("serialize cart");
    assert!(!stored.contains("image"));
}
