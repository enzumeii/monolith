#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::Uri;

fn extract(id: &str, query: &str) -> Result<(ShopifyId, PageRequest), AppError> {
    let uri: Uri = format!("/v1/inventories/{id}?{query}").parse().unwrap();
    extract_inventory_request(Ok(Path(id.into())), Query::try_from_uri(&uri))
}
#[test]
fn defaults_and_explicit_page_values_preserve_the_cursor() {
    let (id, page) = extract("123", "").unwrap();
    assert_eq!(
        serde_json::to_value(id).unwrap(),
        "gid://shopify/Location/123"
    );
    assert_eq!(page.first, 50);
    assert!(page.after.is_none());
    for size in [1, 50, 250] {
        let (_, page) = extract("123", &format!("page_size={size}&cursor=abc%2B%2F%3D")).unwrap();
        assert_eq!(page.first, size);
        assert_eq!(page.after.as_deref(), Some("abc+/="));
    }
}
#[test]
fn malformed_inventory_input_returns_bad_request() {
    for id in ["", "0", "000", "-1", "1.5", "bad"] {
        assert_eq!(
            axum::response::IntoResponse::into_response(extract(id, "").err().unwrap()).status(),
            400
        );
    }
    for query in [
        "page_size=0",
        "page_size=251",
        "page_size=-1",
        "page_size=abc",
        "page_size=65536",
        "cursor=",
        "cursor=%20",
        "unexpected=1",
        "type=INVENTORY",
        "page_size=1&page_size=2",
    ] {
        assert_eq!(
            axum::response::IntoResponse::into_response(extract("1", query).err().unwrap())
                .status(),
            400,
            "{query}"
        );
    }
}
