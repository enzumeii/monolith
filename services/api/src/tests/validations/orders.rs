#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::Uri;
fn extract(query: &str) -> Result<Vec<String>, AppError> {
    let uri: Uri = format!("/v1/admin/orders/report?{query}").parse().unwrap();
    extract_order_query(Query::try_from_uri(&uri))
}
#[test]
fn tags_are_comma_separated_trimmed_literals() {
    assert_eq!(extract("tags=a%3Ab%2Cc%20d").unwrap(), ["a:b", "c d"]);
    assert_eq!(
        extract("tags=%20SHIPPING%20,%20TEST%20").unwrap(),
        ["SHIPPING", "TEST"]
    );
}
#[test]
fn report_rejects_missing_tags_and_pagination() {
    for query in [
        "",
        "tags=",
        "tags=%20",
        "tags=,",
        "tags=shipping,",
        "tags=,shipping",
        "tags=shipping,%20,test",
        "tags=x&page_size=1",
        "tags=x&cursor=next",
        "tags=x&unknown=y",
        "tags=x&tags=y",
    ] {
        assert_eq!(
            axum::response::IntoResponse::into_response(extract(query).unwrap_err()).status(),
            400,
            "{query}"
        );
    }
}

#[test]
fn fulfillment_reuses_shopify_id_bounds_without_changing_the_request_contract() {
    use axum::{Json, extract::Path};
    use domain::dto::orders::{Courier, FulfillOrderRequest};
    for (id, valid) in [
        ("0", false),
        ("-1", false),
        ("１２", false),
        ("18446744073709551616", false),
        ("1", true),
        ("18446744073709551615", true),
    ] {
        let result = extract_fulfill_request(
            Ok(Path(id.into())),
            Ok(Json(FulfillOrderRequest {
                courier: Courier::Best,
                tracking_number: "VP123".into(),
            })),
        );
        assert_eq!(result.is_ok(), valid, "{id}");
        if let Ok((gid, _)) = result {
            assert_eq!(gid.as_ref(), format!("gid://shopify/Order/{id}"));
        }
    }
}

#[test]
fn listing_cursor_can_be_omitted_and_preserves_a_provided_cursor() {
    for (query, cursor) in [
        ("", None),
        ("page_size=1", None),
        ("cursor=next", Some("next")),
    ] {
        let uri: Uri = format!("/v1/admin/orders?{query}").parse().unwrap();
        let (_, page) = extract_list_query(Query::try_from_uri(&uri)).unwrap();
        assert_eq!(page.after.as_deref(), cursor);
        assert_eq!(page.first, if query == "page_size=1" { 1 } else { 50 });
    }
}

#[test]
fn listing_accepts_the_reported_tag_and_fulfillment_query() {
    let uri: Uri = "/v1/admin/orders?tag=shipping&fulfillment=UNFULFILLED&page_size=1"
        .parse()
        .unwrap();
    let (filter, page) = extract_list_query(Query::try_from_uri(&uri)).unwrap();
    assert_eq!(filter.tags, Some(vec!["shipping".into()]));
    assert!(matches!(
        filter.fulfillment,
        Some(domain::dto::orders::FulfillmentStatus::Unfulfilled)
    ));
    assert_eq!(page.first, 1);
    assert!(page.after.is_none());
}

#[test]
fn listing_rejects_blank_tag_and_invalid_pagination_before_business_search() {
    for query in [
        "tag=",
        "tag=%20",
        "tag=x,",
        "tag=,x",
        "tag=x,%20",
        "page_size=0",
        "page_size=251",
        "cursor=%20",
        "fulfillment=unknown",
        "unknown=x",
    ] {
        let uri: Uri = format!("/v1/admin/orders?{query}").parse().unwrap();
        assert!(
            extract_list_query(Query::try_from_uri(&uri)).is_err(),
            "{query}"
        );
    }
}
