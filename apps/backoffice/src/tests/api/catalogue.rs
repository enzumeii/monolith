#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::api::tests::metadata_response;

#[tokio::test]
async fn raw_shopify_search_and_cursor_are_query_encoded_without_reinterpretation() {
    let (result, request) = metadata_response(
        200,
        r#"{"products":[],"page_info":{"has_next_page":true,"end_cursor":"next"}}"#,
        products(CatalogueQuery {
            location_id: "123".into(),
            page_size: Some(25),
            cursor: Some("abc+/=".into()),
            search: Some(" title:Fox AND tag:'a&b' ".into()),
        }),
    )
    .await;
    assert!(result.expect("valid fixture value").page_info.has_next_page);
    let target = request
        .lines()
        .next()
        .expect("valid fixture value")
        .split_whitespace()
        .nth(1)
        .expect("valid fixture value");
    let url =
        reqwest::Url::parse(&format!("https://example.test{target}")).expect("valid fixture value");
    let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(pairs["search"], " title:Fox AND tag:'a&b' ");
    assert_eq!(pairs["cursor"], "abc+/=");
    assert_eq!(url.path(), "/v1/admin/catalogue/products");
    assert!(request.contains("x-session-mode: cookie"));
}
