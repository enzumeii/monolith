#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::api::tests::{metadata_response, metadata_responses};
use domain::dto::{ConventionPaymentMethod, ConventionVariantInput, PatchField};

const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";
const RECORD: &str = r##"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","slug":"event","name":"Event","logo_preview_url":null,"country_code":"TH","event_starts_on":"2026-11-01","event_ends_on":"2026-11-02","description_markdown":"# Event","preorder_enabled":false,"currency_code":"THB","payment_methods":["stripe"],"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"##;
fn row(variant: usize) -> String {
    format!(
        r#"{{"id":"row{variant}","convention_id":"{ID}","shopify_product_id":"1","shopify_variant_id":"{variant}","final_price":"12.99","created_at":"now","updated_at":"now"}}"#
    )
}

#[tokio::test]
async fn complete_memberships_traverse_every_page_before_returning() {
    let first = format!(
        r#"{{"items":[{}],"page":1,"per_page":100,"has_next":true}}"#,
        row(1)
    );
    let second = format!(
        r#"{{"items":[{}],"page":2,"per_page":100,"has_next":false}}"#,
        row(2)
    );
    let (result, requests) =
        metadata_responses(&[(200, &first), (200, &second)], all_variants(ID)).await;
    assert_eq!(result.expect("valid fixture value").len(), 2);
    assert!(requests[0].starts_with(&format!(
        "GET /v1/admin/conventions/{ID}/variants?page=1&per_page=100 HTTP/1.1"
    )));
    assert!(requests[1].starts_with(&format!(
        "GET /v1/admin/conventions/{ID}/variants?page=2&per_page=100 HTTP/1.1"
    )));
    assert!(
        requests
            .iter()
            .all(|value| value.contains("x-session-mode: cookie"))
    );
}
#[tokio::test]
async fn a_failed_or_invalid_later_page_never_exposes_a_partial_selection() {
    let first = format!(
        r#"{{"items":[{}],"page":1,"per_page":100,"has_next":true}}"#,
        row(1)
    );
    let duplicate = format!(
        r#"{{"items":[{}],"page":2,"per_page":100,"has_next":false}}"#,
        row(1)
    );
    for (status, second) in [
        (502, r#"{"error":"Shopify unavailable"}"#),
        (200, duplicate.as_str()),
        (
            200,
            r#"{"items":[],"page":2,"per_page":100,"has_next":true}"#,
        ),
    ] {
        let (result, requests) =
            metadata_responses(&[(200, &first), (status, second)], all_variants(ID)).await;
        assert!(result.is_err());
        assert_eq!(requests.len(), 2);
    }
}
#[tokio::test]
async fn crud_options_logo_and_exact_price_requests_use_the_existing_cookie_transport() {
    let (result, request) = metadata_response(
        200,
        r#"{"countries":["TH"],"currencies":[{"code":"THB","payment_methods":["stripe"]}]}"#,
        options(),
    )
    .await;
    assert_eq!(
        result.expect("valid fixture value").currencies[0].code,
        "THB"
    );
    assert!(request.starts_with("GET /v1/admin/conventions/options HTTP/1.1"));
    let (result, request) = metadata_response(200, RECORD, get(ID)).await;
    assert_eq!(result.expect("valid fixture value").id, ID);
    assert!(request.contains("x-session-mode: cookie"));
    let input = CreateConvention {
        slug: "event".into(),
        name: "Event".into(),
        logo_url: None,
        country_code: "TH".into(),
        event_starts_on: "2026-11-01".into(),
        event_ends_on: "2026-11-02".into(),
        description_markdown: "# Event".into(),
        currency_code: "THB".into(),
        payment_methods: vec![ConventionPaymentMethod::Stripe],
    };
    let (result, request) = metadata_response(201, RECORD, create(input)).await;
    result.expect("valid fixture value");
    assert!(request.starts_with("POST /v1/admin/conventions HTTP/1.1"));
    assert!(request.contains(r#""currency_code":"THB""#));
    let (result, request) = metadata_response(
        200,
        RECORD,
        update(
            ID,
            PatchConvention {
                name: PatchField::Set("Changed".into()),
                ..Default::default()
            },
        ),
    )
    .await;
    result.expect("valid fixture value");
    assert!(request.ends_with(r#"{"name":"Changed"}"#));
    let (result, request) = metadata_response(
        200,
        r#"{"items":[]}"#,
        replace_variants(
            ID,
            ReplaceConventionVariants {
                items: vec![ConventionVariantInput {
                    shopify_product_id: "1".into(),
                    shopify_variant_id: "2".into(),
                    final_price: "12.990".into(),
                }],
            },
        ),
    )
    .await;
    result.expect("valid fixture value");
    assert!(request.contains(r#""final_price":"12.990""#));
}
#[test]
fn signed_logo_preview_urls_allow_only_https_or_literal_local_http() {
    for url in [
        "https://storage.test/image?signature=abc",
        "http://127.0.0.1:8080/image",
        "http://localhost/image",
        "http://[::1]/image",
    ] {
        assert!(logo_preview_url(url).is_some(), "{url}");
    }
    for url in [
        "/api/v1/conventions/key/logo/key",
        "http://storage.test/image",
        "http://2130706433/image",
        "http://0x7f000001/image",
        "https://user:pass@storage.test/image",
        "https://storage.test/image#fragment",
        "//storage.test/image",
    ] {
        assert!(logo_preview_url(url).is_none(), "{url}");
    }
}
#[tokio::test]
async fn unsafe_ids_are_rejected_before_transport() {
    for id in ["../options", "id?x=1", "invalid"] {
        assert_eq!(get(id).await.expect_err("expected rejection").status, 400);
        assert_eq!(
            variants(id, 1)
                .await
                .expect_err("expected rejection")
                .status,
            400
        );
    }
}

#[tokio::test]
async fn membership_pages_must_match_the_record_page_and_page_size() {
    let wrong_record = format!(
        r#"{{"items":[{}],"page":1,"per_page":100,"has_next":false}}"#,
        row(1).replace(ID, "another-convention")
    );
    for body in [
        wrong_record,
        r#"{"items":[],"page":2,"per_page":100,"has_next":false}"#.into(),
        r#"{"items":[],"page":1,"per_page":25,"has_next":false}"#.into(),
    ] {
        let (result, requests) = metadata_response(200, &body, all_variants(ID)).await;
        assert_eq!(result.expect_err("expected rejection").status, 502);
        assert_eq!(
            requests
                .lines()
                .next()
                .expect("valid fixture value")
                .split_whitespace()
                .next()
                .expect("valid fixture value"),
            "GET"
        );
    }
}

#[tokio::test]
async fn selected_variants_decode_enrichment_and_presence_on_get_and_put_without_changing_requests()
{
    let extra = r#", "inventory_policy":"DENY", "product_title":"Fox shirt", "variant_title":"Blue / M", "sku":null, "product_image":{"url":"https://cdn.test/product.png"}, "variant_image":{"url":"https://cdn.test/blue.png"}"#;
    let enriched = format!("{}{extra}}}", row(2).trim_end_matches('}'));
    let page = format!(r#"{{"items":[{enriched}],"page":1,"per_page":100,"has_next":false}}"#);
    let put = format!(r#"{{"items":[{enriched}]}}"#);
    let (rows, _) = metadata_response(200, &page, all_variants(ID)).await;
    let rows = rows.expect("valid fixture value");
    assert_eq!(rows[0].variant_title, PatchField::Set("Blue / M".into()));
    assert_eq!(rows[0].sku, PatchField::Clear);
    assert!(rows[0].available_quantity.is_missing());
    assert_eq!(rows[0].membership.id, "row2");
    assert_eq!(rows[0].membership.convention_id, ID);
    assert_eq!(rows[0].membership.shopify_product_id, "1");
    assert_eq!(rows[0].membership.shopify_variant_id, "2");
    assert_eq!(rows[0].membership.final_price, "12.99");
    assert_eq!(
        rows[0].membership.inventory_policy,
        Some(domain::dto::CatalogueInventoryPolicy::Deny)
    );
    assert!(rows[0].membership.product_title.is_none());
    assert!(rows[0].membership.variant_title.is_none());
    assert!(rows[0].membership.sku.is_none());
    assert!(rows[0].membership.product_image.is_none());
    assert!(rows[0].membership.variant_image.is_none());
    let SelectedVariant {
        _product_image: product_image,
        ..
    } = &rows[0];
    assert!(
        matches!(product_image, PatchField::Set(image) if image.url == "https://cdn.test/product.png")
    );
    assert!(
        matches!(&rows[0].variant_image, PatchField::Set(image) if image.url == "https://cdn.test/blue.png")
    );
    let input = ReplaceConventionVariants {
        items: vec![ConventionVariantInput {
            shopify_product_id: "1".into(),
            shopify_variant_id: "2".into(),
            final_price: "12.990".into(),
        }],
    };
    let (result, request) = metadata_response(200, &put, replace_variants(ID, input)).await;
    let saved = result.expect("valid fixture value").items.remove(0);
    assert_eq!(saved.product_title, PatchField::Set("Fox shirt".into()));
    assert_eq!(saved.variant_title, PatchField::Set("Blue / M".into()));
    assert_eq!(saved.sku, PatchField::Clear);
    assert!(
        matches!(saved.variant_image, PatchField::Set(image) if image.url == "https://cdn.test/blue.png")
    );
    assert_eq!(saved.membership.shopify_product_id, "1");
    assert_eq!(saved.membership.shopify_variant_id, "2");
    assert_eq!(saved.membership.final_price, "12.99");
    assert_eq!(
        saved.membership.inventory_policy,
        Some(domain::dto::CatalogueInventoryPolicy::Deny)
    );
    assert!(saved.available_quantity.is_missing());
    assert!(request.ends_with(
        r#"{"items":[{"shopify_product_id":"1","shopify_variant_id":"2","final_price":"12.990"}]}"#
    ));
    let legacy = format!(
        r#"{{"items":[{}],"page":1,"per_page":100,"has_next":false}}"#,
        row(2)
    );
    let (result, _) = metadata_response(200, &legacy, all_variants(ID)).await;
    let row = result.expect("valid fixture value").remove(0);
    assert!(row.product_title.is_missing());
    assert!(row.variant_title.is_missing());
    let SelectedVariant {
        _product_image: product_image,
        ..
    } = &row;
    assert!(product_image.is_missing());
    assert!(row.membership.inventory_policy.is_none());
    assert!(row.sku.is_missing());
    assert!(row.variant_image.is_missing());
    assert!(row.available_quantity.is_missing());
}

#[tokio::test]
async fn invalid_display_field_types_fail_instead_of_exposing_a_partial_selection() {
    for extra in [
        r#", "sku":42"#,
        r#", "variant_image":"url""#,
        r#", "product_image":"url""#,
        r#", "available_quantity":"0""#,
        r#", "variant_title":false"#,
    ] {
        let row = format!("{}{extra}}}", row(2).trim_end_matches('}'));
        let body = format!(r#"{{"items":[{row}],"page":1,"per_page":100,"has_next":false}}"#);
        let (result, _) = metadata_response(200, &body, all_variants(ID)).await;
        assert!(result.is_err(), "invalid {extra}");
    }
}
