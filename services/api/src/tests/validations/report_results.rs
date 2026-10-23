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
    assert!(result().into_dto().is_ok());
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
        assert!(row.into_dto().is_err(), "mutation {kind}");
    }
    assert!(!validate_csv_filename("file.xlsx"));
}
#[test]
fn metadata_response_does_not_expose_storage_receipts_or_capabilities() {
    let value =
        serde_json::to_value(result().into_dto().expect("valid metadata")).expect("DTO JSON");
    assert_eq!(
        value["files"][0],
        serde_json::json!({"filename":"inventory.csv", "content_type":"text/csv; charset=utf-8"})
    );
}
