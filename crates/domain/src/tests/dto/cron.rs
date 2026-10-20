use crate::dto::{PatchCronConfiguration, PatchField, ScheduleReportType};
use serde_json::json;

#[test]
fn schedule_report_type_uses_the_api_wire_names() {
    assert_eq!(
        serde_json::to_value(ScheduleReportType::Transaction).expect("valid cron fixture"),
        json!("TRANSACTION")
    );
}

#[test]
fn cron_configuration_patch_distinguishes_missing_and_null_fields() {
    let patch: PatchCronConfiguration = serde_json::from_value(json!({
        "period_mode": null,
        "inventory_id": "123"
    }))
    .expect("valid cron fixture");

    assert_eq!(patch.period_mode, PatchField::Clear);
    assert_eq!(patch.inventory_id, PatchField::Set("123".to_owned()));
    assert_eq!(patch.name, PatchField::Missing);
}

#[test]
fn file_format_defaults_to_csv_and_patch_rejects_unknown_formats() {
    let request: crate::dto::CreateCronConfiguration = serde_json::from_value(json!({
        "name":"inventory","report_type":"INVENTORY","cron_expression":"0 9 * * *",
        "receiver_email":"reports@example.test","inventory_id":"1"
    }))
    .expect("valid configuration");
    assert_eq!(request.file_format, crate::dto::ReportFileFormat::Csv);
    let patch: PatchCronConfiguration =
        serde_json::from_value(json!({"file_format":"XLSX"})).expect("valid format");
    assert_eq!(
        patch.file_format,
        PatchField::Set(crate::dto::ReportFileFormat::Xlsx)
    );
    assert!(
        serde_json::from_value::<PatchCronConfiguration>(json!({"file_format":"JSON"})).is_err()
    );
}
