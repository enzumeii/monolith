#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::api::tests::metadata_response;
use domain::dto::{ListReportResultsQuery, ReportStatus, ScheduleReportType};
const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";
const REPORT: &str = r#"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","report_type":"INVENTORY","attempted_at":"2026-10-22T00:00:00Z","generated_at":null,"status":"PENDING","error":null,"parameters":"123","files":[]}"#;

#[tokio::test]
async fn history_preserves_filters_pagination_ids_and_detail_route() {
    let body = format!(r#"{{"items":[{REPORT}],"page":2,"per_page":10,"has_next":true}}"#);
    let (response, request) = metadata_response(
        200,
        &body,
        list_reports(ListReportResultsQuery {
            report_type: Some(ScheduleReportType::Inventory),
            generated_from: Some("2026-10-01T00:00:00+07:00".into()),
            generated_to: Some("2026-11-01T00:00:00+07:00".into()),
            page: Some(2),
            per_page: Some(10),
        }),
    )
    .await;
    let response = response.expect("history");
    assert_eq!(
        (response.page, response.per_page, response.has_next),
        (2, 10, true)
    );
    assert_eq!(response.items[0].id, ID);
    assert!(request.starts_with("GET /v1/admin/cron/reports?report_type=INVENTORY&generated_from=2026-10-01T00%3A00%3A00%2B07%3A00&generated_to=2026-11-01T00%3A00%3A00%2B07%3A00&page=2&per_page=10 HTTP/1.1"));
    assert!(request.contains("x-session-mode: cookie"));
    let (response, request) = metadata_response(200, REPORT, get_report(ID)).await;
    let report = response.expect("detail");
    assert_eq!(report.status, ReportStatus::Pending);
    assert_eq!(report.files, []);
    assert_eq!(report.generated_at, None);
    assert_eq!(report.error, None);
    assert!(request.starts_with(&format!("GET /v1/admin/cron/reports/{ID} HTTP/1.1")));
}

#[tokio::test]
async fn history_reports_safe_errors_invalid_payloads_and_unsafe_ids() {
    for (status, body, want) in [
        (
            404,
            r#"{"error":"Report result not found"}"#,
            "Report result not found",
        ),
        (401, "unauthorized", "Request failed"),
        (200, "not JSON", "Invalid API response"),
    ] {
        let (response, _) = metadata_response(status, body, get_report(ID)).await;
        let error = response.expect_err("error");
        assert_eq!(error.status, if status == 200 { 502 } else { status });
        assert_eq!(error.to_string(), want);
    }
    for id in ["", "../configs", "id/file", "id?download=true"] {
        assert_eq!(get_report(id).await.expect_err("invalid ID").status, 400);
    }
}
