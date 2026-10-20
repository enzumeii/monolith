use super::*;
use axum::{extract::Query, http::Uri, response::IntoResponse};

fn extract(query: &str) -> Result<AccountingDateRange, AppError> {
    let uri: Uri = format!("/v1/admin/transactions?{query}")
        .parse()
        .expect("query URI is valid");
    extract_accounting_query(Query::try_from_uri(&uri))
}

#[test]
fn accounting_report_requires_rfc3339_bounds_with_forward_range() {
    for query in [
        "",
        "date_from=2026-10-01T00%3A00%3A00%2B07%3A00",
        "date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
        "date_from=not-a-date&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
        "date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-01T00%3A00%3A00%2B07%3A00",
        "date_from=2026-11-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
        "date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00&unknown=x",
    ] {
        assert_eq!(
            extract(query)
                .expect_err("invalid query should be rejected")
                .into_response()
                .status(),
            StatusCode::BAD_REQUEST,
            "{query}"
        );
    }
}

#[test]
fn accounting_report_preserves_the_supplied_offsets() {
    let range = extract(
        "date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
    )
    .expect("RFC3339 query should be accepted");
    assert_eq!(range.start.offset().local_minus_utc(), 7 * 3600);
    assert_eq!(range.end.offset().local_minus_utc(), 7 * 3600);
}

#[test]
fn accounting_report_accepts_raw_plus_and_encoded_timezone_offsets() {
    for offset in ["+07:00", "%2B07:00", "%2007:00", "Z", "-03:30"] {
        let range = extract(&format!(
            "date_from=2026-10-01T00:00:00.123{offset}&date_to=2026-10-31T23:59:59{offset}"
        ))
        .expect("valid offset must survive query decoding");
        assert!(range.start < range.end);
    }
    assert!(extract("date_from=2026-10-01T12:00:00+07:00&date_to=2026-10-01T06:00:00Z").is_ok());
    assert!(extract("date_from=2026-10-01T12:00:00+07:00&date_to=2026-10-01T05:00:00Z").is_err());
}
