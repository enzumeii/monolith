use super::*;
use crate::models::cron::report::{ReportResult, SavedFile};
fn result() -> ReportResult {
    let id = uuid::Uuid::new_v4();
    ReportResult {
        id,
        report_type: "INVENTORY".into(),
        attempted_at: chrono::Utc::now().naive_utc(),
        generated_at: Some(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 22)
                .expect("date")
                .and_hms_opt(0, 0, 0)
                .expect("time"),
        ),
        status: "FAILED".into(),
        error: Some("Generation failed".into()),
        parameters: "123".into(),
        files: vec![SavedFile {
            filename: "inventory.csv".into(),
            content_type: "text/csv; charset=utf-8".into(),
            bucket: "reports".into(),
            storage_key: format!("inventory/20261022/{id}/{}/inventory.csv", "a".repeat(64)),
            size_bytes: 8,
            sha256: "a".repeat(64),
        }],
    }
}
#[test]
fn malformed_saved_metadata_returns_controlled_errors() {
    assert!(result_dto(result()).is_ok());
    for kind in 0..15 {
        let mut row = result();
        match kind {
            0 => row.status = "UNKNOWN".into(),
            1 => row.report_type = "UNKNOWN".into(),
            2 => row.parameters = "invalid".into(),
            3 => row.files[0].filename = String::new(),
            4 => row.files[0].filename = "../file.csv".into(),
            5 => row.files[0].content_type = "application/octet-stream".into(),
            6 => row.files[0].bucket = "../reports".into(),
            7 => row.files[0].sha256 = "b".repeat(64),
            8 => {
                row.files[0].storage_key = row.files[0].storage_key.replace("20261022", "20260231");
            }
            9 => {
                row.files[0].storage_key = row.files[0]
                    .storage_key
                    .replace("inventory/", "transaction/");
            }
            10 => {
                row.files[0].storage_key = row.files[0]
                    .storage_key
                    .replace(&row.id.to_string(), &uuid::Uuid::new_v4().to_string());
            }
            11 => row.files[0].storage_key.push_str("/extra"),
            12 => row.generated_at = None,
            13 => {
                row.files[0].storage_key = row.files[0].storage_key.replace("20261022", "20261021");
            }
            _ => row.files[0].storage_key = "https://untrusted.example.test/inventory.csv".into(),
        }
        assert!(result_dto(row).is_err(), "mutation {kind}");
    }
    assert!(!SavedFile::valid_filename("file.xlsx"));
}
#[test]
fn metadata_response_does_not_expose_storage_receipts_or_capabilities() {
    let value =
        serde_json::to_value(result_dto(result()).expect("valid metadata")).expect("DTO JSON");
    assert_eq!(
        value["files"][0],
        serde_json::json!({"filename":"inventory.csv", "content_type":"text/csv; charset=utf-8"})
    );
}

#[test]
fn saved_transaction_period_retains_bangkok_bounds_and_subsecond_accounting_end() {
    let resolved = resolve_parameters(
        ScheduleReportType::Transaction,
        "PREVIOUS_MONTH",
        chrono::DateTime::parse_from_rfc3339("2026-10-31T17:00:00Z")
            .expect("due")
            .naive_utc(),
    )
    .expect("resolved month");
    assert_eq!(
        resolved,
        "2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00"
    );
    let range = saved_parameters(ScheduleReportType::Transaction, &resolved)
        .expect("saved period")
        .expect("transaction range");
    assert_eq!(range.start.to_rfc3339(), "2026-10-01T00:00:00+07:00");
    assert_eq!(
        range.end.to_rfc3339(),
        "2026-10-31T23:59:59.999999999+07:00"
    );
    for value in [
        "2026-10-01T00:00:00Z - 2026-10-31T23:59:59Z",
        "2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59.5+07:00",
        "2026-10-01T01:00:00+07:00 - 2026-10-31T23:59:59+07:00",
        "2026-10-01T00:00:00+07:00 - 2026-11-01T00:00:00+07:00",
        "2026-11-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00",
    ] {
        let error = saved_parameters(ScheduleReportType::Transaction, value)
            .expect_err("invalid saved period");
        assert_eq!(error.to_string(), "Saved report metadata is invalid");
    }
}
