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
