use crate::dto::{
    CreateCronConfiguration, PatchCronConfiguration, PatchField, PreviewCronConfiguration,
};
use serde_json::json;

#[test]
fn create_defaults_disabled_and_uses_one_parameter() {
    let request: CreateCronConfiguration=serde_json::from_value(json!({"name":"inventory","report_type":"INVENTORY","parameter":"123","cron_expression":"0 9 * * *","receiver_email":"reports@example.test"})).expect("valid cron request");
    assert!(!request.enabled);
    assert_eq!(request.parameter, "123");
}
#[test]
fn patch_distinguishes_missing_clear_and_false() {
    let patch: PatchCronConfiguration =
        serde_json::from_value(json!({"enabled":false,"parameter":null}))
            .expect("valid cron request");
    assert_eq!(patch.enabled, PatchField::Set(false));
    assert_eq!(patch.parameter, PatchField::Clear);
    assert_eq!(patch.name, PatchField::Missing);
}
#[test]
fn preview_rejects_legacy_format_or_range_fields() {
    for field in ["file_format", "period_mode", "inventory_id", "status"] {
        let mut body =
            json!({"report_type":"INVENTORY","cron_expression":"0 9 * * *","parameter":"123"});
        body[field] = json!("old");
        assert!(serde_json::from_value::<PreviewCronConfiguration>(body).is_err());
    }
}
