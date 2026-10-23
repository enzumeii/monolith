use super::*;
#[test]
fn zero_stock_is_distinct_from_an_unloaded_inventory_level() {
    assert_eq!(stock_label(Some(0)), "0");
    assert_eq!(stock_label(None), "Missing or inactive inventory level");
    assert_eq!(stock_label(Some(-2)), "-2");
}
#[test]
fn provider_images_are_safe_https_urls_only() {
    assert!(safe_image_url("https://cdn.example.test/image.png"));
    for value in [
        "javascript:alert(1)",
        "data:image/svg+xml,<svg>",
        "//cdn.example.test/image",
        "https://user:password@cdn.example.test/a",
    ] {
        assert!(!safe_image_url(value));
    }
}

#[test]
fn catalogue_submission_converts_numeric_location_and_omits_empty_search() {
    let query = BrowseQuery {
        location: "123".into(),
        ..Default::default()
    }
    .to_api_query();
    assert_eq!(query.location_id, "gid://shopify/Location/123");
    assert_eq!(query.page_size, Some(25));
    assert_eq!(query.search, None);
    assert_eq!(query.cursor, None);
}

#[test]
fn catalogue_submission_preserves_nonempty_search_and_pagination_cursor() {
    let query = BrowseQuery {
        location: "1".into(),
        search: " title:Fox AND tag:'a&b' ".into(),
        cursor: Some("abc+/=".into()),
    }
    .to_api_query();
    assert_eq!(query.location_id, "gid://shopify/Location/1");
    assert_eq!(query.search.as_deref(), Some(" title:Fox AND tag:'a&b' "));
    assert_eq!(query.cursor.as_deref(), Some("abc+/="));
}
