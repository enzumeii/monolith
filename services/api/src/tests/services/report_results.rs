#![allow(clippy::unwrap_used)]

use crate::services::report_results::{make_filter, parse_timestamp};
use domain::dto::ListReportResultsQuery;

#[test]
fn result_filters_require_offset_timestamps_and_use_exclusive_upper_bounds() {
    let query: ListReportResultsQuery = serde_json::from_value(serde_json::json!({
        "generated_from": "2026-10-01T00:00:00+07:00",
        "generated_to": "2026-11-01T00:00:00+07:00"
    }))
    .unwrap();
    let filter = make_filter(&query).unwrap();
    assert_eq!(filter.page, 1);
    assert_eq!(filter.per_page, 25);
    assert_eq!(
        parse_timestamp("2026-10-01T00:00:00+07:00")
            .unwrap()
            .to_string(),
        "2026-09-30 17:00:00"
    );
    assert!(parse_timestamp("2026-10-01T00:00:00").is_err());
}
