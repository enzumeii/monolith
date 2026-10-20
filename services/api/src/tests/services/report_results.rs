#![allow(clippy::unwrap_used)]

use crate::services::report_results::{
    csv_from_saved, make_filter, parse_timestamp, safe_filename,
};
use domain::dto::ListReportResultsQuery;
use serde_json::json;

#[test]
fn result_filters_require_offset_timestamps_and_use_exclusive_upper_bounds() {
    let query = ListReportResultsQuery {
        configuration_id: None,
        report_type: None,
        generation_status: None,
        email_status: None,
        scheduled_from: Some("2026-10-01T00:00:00+07:00".to_owned()),
        scheduled_to: Some("2026-11-01T00:00:00+07:00".to_owned()),
        page: None,
        per_page: None,
    };
    let filter = make_filter(query).unwrap();
    assert_eq!(
        filter.scheduled_from.unwrap().to_string(),
        "2026-09-30 17:00:00"
    );
    assert_eq!(
        filter.scheduled_to.unwrap().to_string(),
        "2026-10-31 17:00:00"
    );
    assert!(parse_timestamp("2026-10-01T00:00:00").is_err());
}

#[test]
fn saved_inventory_csv_uses_stable_columns_and_escapes_untrusted_text() {
    let payload = json!({
        "inventory": [{
            "sku": "=HYPERLINK(\"https://example.test\")",
            "title": "Hammer, large",
            "selling_price": -4.5,
            "quantity": 2
        }]
    });
    let csv = csv_from_saved("INVENTORY", &payload, "inventory").unwrap();
    assert!(csv.starts_with("category,collection,sku,title,"));
    assert!(csv.contains("'=HYPERLINK(\"\"https://example.test\"\")"));
    assert!(csv.contains("\"Hammer, large\""));
    assert!(csv.ends_with(",,-4.5,,2,,\r\n"));
}

#[test]
fn report_filenames_strip_path_separators_and_control_characters() {
    assert_eq!(
        safe_filename("../monthly\nreport", "inventory"),
        "monthly-report-inventory.csv"
    );
}
