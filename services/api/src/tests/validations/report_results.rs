use super::*;
use crate::models::cron::report::{ReportResult, SavedFile};
use sqlx::types::Json;
fn result() -> ReportResult {
    ReportResult {
        id: uuid::Uuid::new_v4(),
        report_type: "INVENTORY".into(),
        attempted_at: chrono::Utc::now().naive_utc(),
        generated_at: None,
        status: "FAILED".into(),
        error: Some("Generation failed".into()),
        parameters: "123".into(),
        files: Json(vec![SavedFile {
            filename: "inventory.csv".into(),
            content_type: "text/csv; charset=utf-8".into(),
            download_url: "https://reports.example.test/file.csv".into(),
            storage_key: "private".into(),
        }]),
    }
}
#[test]
fn malformed_saved_metadata_returns_controlled_errors() {
    assert!(result().into_dto().is_ok());
    for kind in 0..7 {
        let mut row = result();
        match kind {
            0 => row.status = "UNKNOWN".into(),
            1 => row.report_type = "UNKNOWN".into(),
            2 => row.parameters = "invalid".into(),
            3 => row.files.0[0].filename = String::new(),
            4 => row.files.0[0].filename = "../file.csv".into(),
            5 => row.files.0[0].content_type = "application/octet-stream".into(),
            _ => {
                row.files.0[0].download_url =
                    "https://user:password@reports.example.test/file.csv".into();
            }
        }
        assert!(row.into_dto().is_err());
    }
    assert!(!validate_csv_filename("file.xlsx"));
}
