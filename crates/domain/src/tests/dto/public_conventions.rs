#![allow(clippy::unwrap_used)]
use crate::dto::{
    ConventionQuote, ListPublicConventionsQuery, PublicCatalogueResponse,
    PublicConventionListResponse, QuoteConventionRequest,
};
use serde_json::json;

#[test]
fn public_directory_round_trips_only_customer_convention_metadata() {
    let wire = json!({
        "items": [{
            "id": "convention-id", "slug": "bangkok-fair", "name": "Bangkok Fair",
            "logo_preview_url": "https://storage.test/logo.png?signed=1",
            "logo_preview_url_expires_at": "2026-10-24T12:00:00Z", "country_code": "TH",
            "event_starts_on": "2026-10-24", "event_ends_on": "2026-10-25",
            "description_markdown": "**Welcome**", "currency_code": "THB"
        }],
        "page": 1, "per_page": 100, "has_next": false, "visitor_country": "TH"
    });
    let response: PublicConventionListResponse = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(response.clone()).unwrap(), wire);
    assert_eq!(response, response.clone());
    let fields = serde_json::to_value(&response.items[0]).unwrap();
    for private_field in [
        "logo_url",
        "preorder_enabled",
        "payment_methods",
        "created_at",
        "updated_at",
    ] {
        assert!(
            fields.get(private_field).is_none(),
            "unexpected {private_field}"
        );
    }
}

#[test]
fn public_catalogue_preserves_final_price_stock_and_selected_options() {
    let wire = json!({
        "convention": {
            "id": "convention-id", "slug": "bangkok-fair", "name": "Bangkok Fair",
            "logo_preview_url": null, "logo_preview_url_expires_at": null,
            "country_code": "TH", "event_starts_on": "2026-10-24",
            "event_ends_on": "2026-10-25", "description_markdown": "", "currency_code": "THB"
        },
        "payment_methods": ["stripe", "paypal"],
        "products": [{
            "id": "gid://shopify/Product/1", "title": "Print", "description": "Art print",
            "product_type": "Prints", "featured_image": {"url": "https://images.test/print.png"},
            "variants": [{
                "id": "gid://shopify/ProductVariant/2", "title": "A4", "sku": "PRINT-A4",
                "image": null, "selected_options": [{"name": "Size", "value": "A4"}],
                "final_price": "100.00", "inventory_policy": "CONTINUE",
                "available_quantity": -1, "max_quantity": 999
            }]
        }]
    });
    let response: PublicCatalogueResponse = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(response.clone()).unwrap(), wire);
    assert_eq!(response, response.clone());
    let fields = serde_json::to_value(&response.products[0].variants[0]).unwrap();
    for private_field in ["price", "supplier_cost", "inventory_item_id", "tracked"] {
        assert!(
            fields.get(private_field).is_none(),
            "unexpected {private_field}"
        );
    }
}

#[test]
fn public_directory_query_requires_date_and_rejects_unknown_fields() {
    let minimal: ListPublicConventionsQuery =
        serde_json::from_value(json!({"today": "2026-10-24"})).unwrap();
    assert_eq!(minimal.locale_country, None);
    assert_eq!(minimal.page, None);
    assert_eq!(minimal.per_page, None);
    assert!(serde_json::from_value::<ListPublicConventionsQuery>(json!({})).is_err());
    assert!(
        serde_json::from_value::<ListPublicConventionsQuery>(json!({
            "today": "2026-10-24", "preorder_enabled": false
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ListPublicConventionsQuery>(json!({
            "today": "2026-10-24", "page": -1
        }))
        .is_err()
    );
}

#[test]
fn quote_request_accepts_empty_cart_and_only_variant_quantities() {
    let empty: QuoteConventionRequest = serde_json::from_value(json!({"items": []})).unwrap();
    assert_eq!(empty.items, Vec::new());
    let wire = json!({"items": [{"variant_id": "gid://shopify/ProductVariant/2", "quantity": 2}]});
    let request: QuoteConventionRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), wire);
    for invalid in [
        json!({"items": [], "currency_code": "THB"}),
        json!({"items": [{"variant_id": "v", "quantity": 2, "unit_price": "0.01"}]}),
        json!({"items": [{"variant_id": "v", "quantity": 1.5}]}),
        json!({"items": [{"variant_id": "v", "quantity": -1}]}),
        json!({"items": [{"variant_id": "v", "quantity": "2"}]}),
        json!({"items": [{"variant_id": "v"}]}),
    ] {
        assert!(
            serde_json::from_value::<QuoteConventionRequest>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn quote_amounts_round_trip_as_exact_strings_including_zero_shipping() {
    for wire in [
        json!({"currency_code": "JPY", "items": [], "subtotal": "0", "shipping": "0", "total": "0"}),
        json!({
            "currency_code": "BHD",
            "items": [{"variant_id": "v", "quantity": 2, "unit_price": "1.235", "line_total": "2.470"}],
            "subtotal": "2.470", "shipping": "0.000", "total": "2.470"
        }),
    ] {
        let quote: ConventionQuote = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(quote).unwrap(), wire);
    }
    assert!(
        serde_json::from_value::<ConventionQuote>(json!({
            "currency_code": "USD", "items": [], "subtotal": 0, "shipping": "0.00", "total": "0.00"
        }))
        .is_err()
    );
}

#[test]
fn missing_checkout_capabilities_fail_closed_for_older_catalogue_responses() {
    let wire = json!({"convention":{"id":"id","slug":"fair","name":"Fair","logo_preview_url":null,"logo_preview_url_expires_at":null,"country_code":"TH","event_starts_on":"2026-10-26","event_ends_on":"2026-10-27","description_markdown":"","currency_code":"THB"},"products":[]});
    let response: PublicCatalogueResponse = serde_json::from_value(wire).unwrap();
    assert_eq!(response.payment_methods, []);
}
