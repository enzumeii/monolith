#![allow(clippy::unwrap_used)]
use super::*;
use axum::http::Uri;
fn extract(query: &str) -> Result<String, AppError> {
    let uri: Uri = format!("/v1/orders/report?{query}").parse().unwrap();
    extract_order_query(Query::try_from_uri(&uri))
}
#[test]
fn tags_is_one_literal_value() {
    assert_eq!(extract("tags=a%3Ab%2Cc%20d").unwrap(), "a:b,c d");
}
#[test]
fn report_rejects_missing_tags_and_pagination() {
    for query in [
        "",
        "tags=",
        "tags=%20",
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
