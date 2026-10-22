use crate::dto::{ListReportResultsQuery, ReportResultDTO, ReportStatus};
use serde_json::json;

#[test]
fn report_metadata_decodes_every_status_nullable_fields_and_saved_files() {
    for (status, expected) in [
        ("PENDING", ReportStatus::Pending),
        ("SUCCESS", ReportStatus::Success),
        ("FAILED", ReportStatus::Failed),
    ] {
        let body = json!({"id":"da41d46b-f1ca-4a76-a774-62c269d28731","report_type":"INVENTORY","attempted_at":"2026-10-22T00:00:00Z","generated_at":null,"status":status,"error":null,"parameters":"123","files":[]});
        let result: ReportResultDTO = serde_json::from_value(body.clone()).expect("result DTO");
        assert_eq!(result.status, expected);
        assert_eq!(result.generated_at, None);
        assert_eq!(result.error, None);
        assert_eq!(result.files, []);
        assert_eq!(serde_json::to_value(result).expect("result JSON"), body);
    }
    let body = json!({"id":"da41d46b-f1ca-4a76-a774-62c269d28731","report_type":"TRANSACTION","attempted_at":"2026-10-22T00:00:00Z","generated_at":"2026-10-22T00:00:05Z","status":"FAILED","error":"Email unavailable","parameters":"2026-09","files":[{"filename":"transactions.csv","content_type":"text/csv","download_url":"https://files.example.test/transactions.csv"}]});
    let result: ReportResultDTO = serde_json::from_value(body.clone()).expect("saved report");
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.error.as_deref(), Some("Email unavailable"));
    assert_eq!(serde_json::to_value(result).expect("result JSON"), body);
    let query = ListReportResultsQuery {
        report_type: None,
        generated_from: Some("2026-10-01T00:00:00+07:00".into()),
        generated_to: None,
        page: Some(1),
        per_page: Some(25),
    };
    assert_eq!(serde_json::to_value(query).expect("query")["page"], 1);
}
