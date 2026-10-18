#![allow(clippy::unwrap_used)]
use super::*;
use axum::{http::Uri, response::IntoResponse};

fn extract(id: &str, query: &str) -> Result<(ShopifyId, PageRequest), AppError> {
    let uri: Uri = format!("/v1/admin/inventories/123?{query}")
        .parse()
        .unwrap();
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

#[test]
fn both_inventory_extractors_share_location_id_validation() {
    for id in ["1", "123", "00123", "18446744073709551615"] {
        let (report_id, _) = extract(id, "").unwrap();
        for query in [None, Some("")] {
            let enable_id = extract_enable_request(Ok(Path(id.into())), query).unwrap();
            assert_eq!(
                serde_json::to_value(&report_id).unwrap(),
                serde_json::to_value(enable_id).unwrap()
            );
            assert_eq!(
                serde_json::to_value(&report_id).unwrap(),
                format!("gid://shopify/Location/{id}")
            );
        }
    }
    for id in [
        "",
        "0",
        "000",
        "-1",
        "+1",
        "1.5",
        "bad",
        " 1",
        "1 ",
        "１２３",
        "١٢٣",
    ] {
        let report = extract(id, "").err().unwrap();
        assert_eq!(report.into_response().status(), 400);
        let enable = extract_enable_request(Ok(Path(id.into())), None)
            .err()
            .unwrap();
        assert_eq!(enable.into_response().status(), 400);
    }
    for query in ["page_size=1", "cursor=abc", "unexpected=1"] {
        assert_eq!(
            extract_enable_request(Ok(Path("1".into())), Some(query))
                .err()
                .unwrap()
                .into_response()
                .status(),
            400
        );
    }
}
