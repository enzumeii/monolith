use super::*;

#[test]
fn only_canonical_slugs_can_reach_customer_endpoints() {
    for slug in [
        "",
        "Uppercase",
        "-fair",
        "fair-",
        "a/b",
        "../admins",
        "fair?query",
        "with space",
        "évent",
        &"a".repeat(101),
    ] {
        let error = validate_slug(slug).expect_err("invalid route slug");
        assert_eq!(error.status, 404, "{slug}");
        assert_eq!(
            error.message,
            "This convention is not available for preorders."
        );
    }
    for slug in ["a", "fair-2026", &"a".repeat(100)] {
        validate_slug(slug).expect("canonical slug");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn invalid_slugs_are_rejected_before_any_network_or_configuration_access() {
    assert_eq!(
        catalogue("../admins")
            .await
            .expect_err("invalid slug")
            .status,
        404
    );
    assert_eq!(
        quote("UPPER", QuoteConventionRequest { items: vec![] })
            .await
            .expect_err("invalid slug")
            .status,
        404
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn directory_collects_all_pages_in_server_priority_order_with_country_hint() {
    let first = page(&["a", "b"], 1, true);
    let second = page(&["c"], 2, false);
    let (result, requests) = crate::api::tests::responses(
        &[(200, &first), (200, &second)],
        directory("2099-01-01".into(), Some("TH".into())),
    )
    .await;
    let conventions = result.expect("complete directory");
    assert_eq!(
        conventions
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        let query = request_query(request);
        assert_eq!(query.get("today").map(String::as_str), Some("2099-01-01"));
        assert_eq!(query.get("locale_country").map(String::as_str), Some("TH"));
        assert_eq!(query.get("page"), Some(&(index + 1).to_string()));
        assert_eq!(query.get("per_page").map(String::as_str), Some("100"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn rejected_locale_restarts_from_page_one_without_stale_items_or_duplicate_ids() {
    let first = page(&["a"], 1, true);
    let reset = page(&["a", "b"], 1, false);
    let (result, requests) = crate::api::tests::responses(
        &[
            (200, &first),
            (400, r#"{"error":"Unsupported locale"}"#),
            (200, &reset),
        ],
        directory("2099-01-01".into(), Some("ZZ".into())),
    )
    .await;
    assert_eq!(
        result
            .expect("country-free retry")
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(requests.len(), 3);
    assert_eq!(
        request_query(&requests[1]).get("page").map(String::as_str),
        Some("2")
    );
    let retry = request_query(&requests[2]);
    assert_eq!(retry.get("page").map(String::as_str), Some("1"));
    assert!(!retry.contains_key("locale_country"));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn directory_does_not_retry_unrelated_errors_or_repeat_a_country_free_rejection() {
    for (statuses, hint, expected_status) in [
        (vec![400], None, 400),
        (vec![503], Some("TH".into()), 503),
        (vec![400, 400], Some("TH".into()), 400),
    ] {
        let fixtures: Vec<_> = statuses
            .iter()
            .map(|status| (*status, r#"{"error":"Directory unavailable"}"#))
            .collect();
        let (result, requests) =
            crate::api::tests::responses(&fixtures, directory("2099-01-01".into(), hint)).await;
        assert_eq!(
            result.expect_err("rejection returned").status,
            expected_status
        );
        assert_eq!(requests.len(), statuses.len());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn inconsistent_directory_pages_fail_instead_of_hiding_or_duplicating_conventions() {
    let first = page(&["a"], 1, true);
    for second in [
        page(&["b"], 1, false),
        page(&["a"], 2, false),
        page(&[], 2, true),
    ] {
        let (result, requests) = crate::api::tests::responses(
            &[(200, &first), (200, &second)],
            directory("2099-01-01".into(), None),
        )
        .await;
        let error = result.expect_err("inconsistent pagination");
        assert_eq!(error.status, 502);
        assert_eq!(
            error.message,
            "The convention list changed while loading. Please retry."
        );
        assert_eq!(requests.len(), 2);
    }
    let empty = page(&[], 1, false);
    let (result, requests) =
        crate::api::tests::responses(&[(200, &empty)], directory("2099-01-01".into(), None)).await;
    assert_eq!(result.expect("empty directory"), Vec::new());
    assert_eq!(requests.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn catalogue_and_quote_use_anonymous_slug_scoped_contracts_and_selected_quantities() {
    let catalogue = crate::api::tests::catalogue();
    let catalogue_json = serde_json::to_string(&catalogue).expect("catalogue JSON");
    let quote_json = r#"{"currency_code":"USD","items":[{"variant_id":"gid://shopify/ProductVariant/1","quantity":2,"unit_price":"12.30","line_total":"24.60"}],"subtotal":"24.60","shipping":"0.00","total":"24.60"}"#;
    let (result, requests) =
        crate::api::tests::responses(&[(200, &catalogue_json), (200, quote_json)], async {
            let loaded = super::catalogue("fair-a").await.expect("catalogue");
            let total = quote(
                "fair-a",
                QuoteConventionRequest {
                    items: vec![domain::dto::QuoteConventionItem {
                        variant_id: loaded.products[0].variants[0].id.clone(),
                        quantity: 2,
                    }],
                },
            )
            .await
            .expect("quote");
            (loaded, total)
        })
        .await;
    assert_eq!(result.0, catalogue);
    assert_eq!(result.1.total, "24.60");
    assert_eq!(result.1.items[0].quantity, 2);
    assert!(requests[0].starts_with("GET /v1/conventions/fair-a/products HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("POST /v1/conventions/fair-a/quote HTTP/1.1\r\n"));
    let (_, body) = requests[1].split_once("\r\n\r\n").expect("request body");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).expect("JSON body"),
        serde_json::json!({"items":[{"variant_id":"gid://shopify/ProductVariant/1","quantity":2}]})
    );
    for request in requests {
        let headers = request
            .split("\r\n\r\n")
            .next()
            .expect("headers")
            .to_ascii_lowercase();
        assert!(!headers.contains("authorization:"));
        assert!(!headers.contains("cookie:"));
        assert!(!headers.contains("x-session-mode:"));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn page(ids: &[&str], page: u32, has_next: bool) -> String {
    serde_json::to_string(&PublicConventionListResponse {
        items: ids
            .iter()
            .map(|id| crate::api::tests::convention(id))
            .collect(),
        page,
        per_page: 100,
        has_next,
        visitor_country: Some("TH".into()),
    })
    .expect("directory JSON")
}

#[cfg(not(target_arch = "wasm32"))]
fn request_query(request: &str) -> std::collections::HashMap<String, String> {
    let target = request
        .split_ascii_whitespace()
        .nth(1)
        .expect("request target");
    let url = reqwest::Url::parse(&format!("http://example.test{target}")).expect("request URL");
    assert_eq!(url.path(), "/v1/conventions");
    url.query_pairs().into_owned().collect()
}
