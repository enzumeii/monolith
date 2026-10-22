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

#[test]
fn patch_serializes_only_supplied_fields_and_retains_null_and_false() {
    let patch = PatchCronConfiguration {
        parameter: PatchField::Clear,
        enabled: PatchField::Set(false),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&patch).expect("patch JSON"),
        json!({"parameter":null,"enabled":false})
    );
    let parsed: PatchCronConfiguration =
        serde_json::from_value(serde_json::to_value(patch).expect("JSON")).expect("patch");
    assert_eq!(parsed.name, PatchField::Missing);
    assert_eq!(parsed.parameter, PatchField::Clear);
    assert_eq!(parsed.enabled, PatchField::Set(false));
}

#[test]
fn schedule_requests_and_responses_roundtrip_without_legacy_fields() {
    use crate::dto::{
        CronConfigurationDTO, CronConfigurationListResponse, CronConfigurationPreview,
        ListCronConfigurationsQuery,
    };
    let create_json = json!({"name":"Monthly","report_type":"TRANSACTION","cron_expression":"0 9 1 * *","receiver_email":"reports@example.test","parameter":"PREVIOUS_MONTH","enabled":false});
    let create: CreateCronConfiguration =
        serde_json::from_value(create_json.clone()).expect("create");
    assert_eq!(
        serde_json::to_value(create).expect("request JSON"),
        create_json
    );
    let preview: PreviewCronConfiguration=serde_json::from_value(json!({"report_type":"TRANSACTION","cron_expression":"0 9 1 * *","parameter":"PREVIOUS_MONTH"})).expect("preview request");
    assert_eq!(
        serde_json::to_value(preview)
            .expect("preview JSON")
            .as_object()
            .expect("object")
            .len(),
        3
    );
    let query = ListCronConfigurationsQuery {
        report_type: None,
        enabled: Some(false),
        page: Some(2),
        per_page: Some(25),
    };
    assert_eq!(
        serde_json::to_value(query).expect("query")["enabled"],
        false
    );
    let dto_json = json!({"id":"da41d46b-f1ca-4a76-a774-62c269d28731","name":"Monthly","report_type":"TRANSACTION","cron_expression":"0 9 1 * *","receiver_email":"reports@example.test","parameter":"PREVIOUS_MONTH","enabled":false,"schedule_timezone":"Asia/Bangkok","created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"});
    let dto: CronConfigurationDTO = serde_json::from_value(dto_json.clone()).expect("DTO");
    assert_eq!(serde_json::to_value(dto).expect("DTO JSON"), dto_json);
    let list: CronConfigurationListResponse =
        serde_json::from_value(json!({"items":[dto_json],"page":2,"per_page":25,"has_next":false}))
            .expect("list");
    assert_eq!(list.items.len(), 1);
    let preview: CronConfigurationPreview=serde_json::from_value(json!({"schedule_timezone":"Asia/Bangkok","description":"Monthly","occurrences":[{"scheduled_for":"2026-11-01T02:00:00Z","period":{"period_start":"2026-10-01T00:00:00+07:00","period_end_exclusive":"2026-11-01T00:00:00+07:00"}},{"scheduled_for":"2026-12-01T02:00:00Z","period":null}]})).expect("preview");
    assert!(preview.occurrences[0].period.is_some());
    assert!(preview.occurrences[1].period.is_none());
}
