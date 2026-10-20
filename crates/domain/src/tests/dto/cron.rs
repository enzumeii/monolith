use crate::dto::{PatchCronConfiguration, PatchField, ScheduleReportType};
use serde_json::json;

#[test]
fn schedule_report_type_uses_the_api_wire_names() {
    assert_eq!(
        serde_json::to_value(ScheduleReportType::Transaction).unwrap(),
        json!("TRANSACTION")
    );
}

#[test]
fn cron_configuration_patch_distinguishes_missing_and_null_fields() {
    let patch: PatchCronConfiguration = serde_json::from_value(json!({
        "period_mode": null,
        "inventory_id": "123"
    }))
    .unwrap();

    assert_eq!(patch.period_mode, PatchField::Clear);
    assert_eq!(patch.inventory_id, PatchField::Set("123".to_owned()));
    assert_eq!(patch.name, PatchField::Missing);
}
