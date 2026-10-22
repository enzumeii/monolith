use super::*;
fn report(status: ReportStatus, generated: bool, error: Option<&str>) -> ReportResultDTO {
    ReportResultDTO {
        id: "audit".into(),
        report_type: domain::dto::ScheduleReportType::Inventory,
        attempted_at: "2026-10-22T00:00:00Z".into(),
        generated_at: generated.then(|| "2026-10-22T00:00:01Z".into()),
        status,
        error: error.map(str::to_owned),
        parameters: "123".into(),
        files: vec![],
    }
}
#[test]
fn workflow_outcome_retains_generation_delivery_publication_and_cancellation_errors() {
    for (generated, error) in [
        (false, "Generation failed"),
        (true, "Email failed"),
        (true, "Email delivery unconfirmed"),
        (true, "Publication failed"),
        (false, "Cancelled"),
    ] {
        let report = report(ReportStatus::Failed, generated, Some(error));
        let outcome = outcome(&report);
        assert_eq!(outcome.label, "FAILED");
        assert_eq!(outcome.error, Some(error));
        assert_eq!(report.files, []);
    }
    let retry = report(
        ReportStatus::Pending,
        true,
        Some("Email delivery unconfirmed"),
    );
    assert_eq!(outcome(&retry).label, "PENDING");
    assert_eq!(outcome(&retry).error, Some("Email delivery unconfirmed"));
    assert!(outcome(&retry).explanation.contains("retry"));
    let completed = report(ReportStatus::Success, true, None);
    assert!(outcome(&completed).explanation.contains("delivery"));
    assert_eq!(outcome(&completed).error, None);
}
