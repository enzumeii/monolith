#![allow(clippy::unwrap_used)]
use super::*;
use axum::{http::Uri, response::IntoResponse};

fn extract(query: &str) -> Result<(ShopifyId, PageRequest, Option<String>), AppError> {
    let uri: Uri = format!("/v1/admin/conventions/catalogue?{query}")
        .parse()
        .unwrap();
    extract_catalogue_request(Query::try_from_uri(&uri))
}

#[test]
fn numeric_locations_become_location_gids_without_changing_opaque_query_values() {
    for id in ["1", "65761018027", "00123", "18446744073709551615"] {
        let (location, page, search) = extract(&format!(
            "location_id={id}&page_size=250&cursor=abc%2B%2F%3D&search=%20product_type%3AT-Shirts%20"
        ))
        .unwrap();
        assert_eq!(location.as_ref(), format!("gid://shopify/Location/{id}"));
        assert_eq!(page.first, 250);
        assert_eq!(page.after.as_deref(), Some("abc+/="));
        assert_eq!(search.as_deref(), Some(" product_type:T-Shirts "));
    }
}

#[test]
fn malformed_zero_overflow_and_gid_locations_are_rejected() {
    for id in [
        "",
        "0",
        "000",
        "-1",
        "%2B1",
        "1.5",
        "abc",
        "%201",
        "1%20",
        "18446744073709551616",
        "gid://shopify/Location/1",
        "gid://shopify/Product/1",
    ] {
        assert_eq!(
            extract(&format!("location_id={id}"))
                .err()
                .unwrap()
                .into_response()
                .status(),
            400,
            "{id}"
        );
    }
    for query in ["", "location_id=1&location_id=2"] {
        assert_eq!(extract(query).err().unwrap().into_response().status(), 400);
    }
}
