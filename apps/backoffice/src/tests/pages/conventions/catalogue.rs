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
