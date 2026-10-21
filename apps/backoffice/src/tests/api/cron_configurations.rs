#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::api::tests::metadata_response;
use domain::dto::{
    CreateCronConfiguration, ListCronConfigurationsQuery, PatchCronConfiguration, PatchField,
    PreviewCronConfiguration, ScheduleReportType,
};

const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";
const CONFIG: &str = r#"{"id":"da41d46b-f1ca-4a76-a774-62c269d28731","name":"Monthly","report_type":"TRANSACTION","cron_expression":"0 9 1 * *","receiver_email":"reports@example.test","parameter":"PREVIOUS_MONTH","enabled":false,"schedule_timezone":"Asia/Bangkok","created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"}"#;

#[tokio::test]
async fn configuration_crud_sends_actual_routes_bodies_and_cookie_transport() {
    let body = format!(r#"{{"items":[{CONFIG}],"page":2,"per_page":25,"has_next":true}}"#);
    let (response, request) = metadata_response(
        200,
        &body,
        list_configurations(ListCronConfigurationsQuery {
            report_type: Some(ScheduleReportType::Transaction),
            enabled: Some(false),
            page: Some(2),
            per_page: Some(25),
        }),
    )
    .await;
    let response = response.expect("list");
    assert_eq!(response.items[0].id, ID);
    assert!(response.has_next);
    assert!(request.starts_with("GET /v1/admin/cron/configs?report_type=TRANSACTION&enabled=false&page=2&per_page=25 HTTP/1.1"));
    assert!(request.contains("x-session-mode: cookie"));
    let (response, request) = metadata_response(200, CONFIG, get_configuration(ID)).await;
    assert_eq!(response.expect("get").schedule_timezone, "Asia/Bangkok");
    assert!(request.starts_with(&format!("GET /v1/admin/cron/configs/{ID} HTTP/1.1")));
    let (response, request) = metadata_response(
        201,
        CONFIG,
        create_configuration(CreateCronConfiguration {
            name: "Monthly".into(),
            report_type: ScheduleReportType::Transaction,
            cron_expression: "0 9 1 * *".into(),
            receiver_email: "reports@example.test".into(),
            parameter: "PREVIOUS_MONTH".into(),
            enabled: false,
        }),
    )
    .await;
    assert_eq!(response.expect("created").id, ID);
    assert!(request.starts_with("POST /v1/admin/cron/configs HTTP/1.1"));
    assert!(request.contains(r#""enabled":false"#));
    assert!(!request.contains("file_format"));
    let (response, request) = metadata_response(
        200,
        CONFIG,
        update_configuration(
            ID,
            PatchCronConfiguration {
                enabled: PatchField::Set(false),
                ..Default::default()
            },
        ),
    )
    .await;
    assert!(!response.expect("updated").enabled);
    assert!(request.starts_with(&format!("PATCH /v1/admin/cron/configs/{ID} HTTP/1.1")));
    assert!(request.ends_with(r#"{"enabled":false}"#));
    let (response, request) = metadata_response(204, "", delete_configuration(ID)).await;
    response.expect("deleted without JSON");
    assert!(request.starts_with(&format!("DELETE /v1/admin/cron/configs/{ID} HTTP/1.1")));
}

#[tokio::test]
async fn preview_is_bodyless_get_with_encoded_query_and_nullable_period() {
    let body = r#"{"schedule_timezone":"Asia/Bangkok","description":"Inventory","occurrences":[{"scheduled_for":"2026-10-23T02:00:00Z","period":null}]}"#;
    let (response, request) = metadata_response(
        200,
        body,
        preview_configuration(PreviewCronConfiguration {
            report_type: ScheduleReportType::Inventory,
            cron_expression: "0 9 * * *".into(),
            parameter: "1&2+3".into(),
        }),
    )
    .await;
    assert!(response.expect("preview").occurrences[0].period.is_none());
    assert!(request.starts_with("GET /v1/admin/cron/configs/preview?report_type=INVENTORY&cron_expression=0+9+*+*+*&parameter=1%262%2B3 HTTP/1.1"));
    assert!(request.ends_with("\r\n\r\n"));
}

#[tokio::test]
async fn configuration_errors_keep_safe_api_messages_and_reject_unsafe_ids() {
    let (result, _) = metadata_response(
        400,
        r#"{"error":"Required fields cannot be null"}"#,
        update_configuration(
            ID,
            PatchCronConfiguration {
                parameter: PatchField::Clear,
                ..Default::default()
            },
        ),
    )
    .await;
    let error = result.expect_err("invalid patch");
    assert_eq!(error.status, 400);
    assert_eq!(error.to_string(), "Required fields cannot be null");
    let (result, _) = metadata_response(
        404,
        r#"{"error":"Cron configuration not found"}"#,
        delete_configuration(ID),
    )
    .await;
    assert_eq!(result.expect_err("missing").status, 404);
    for id in [
        "",
        "../reports",
        "preview",
        "id?enabled=true",
        "da41d46b-f1ca-4a76-a774-62c269d2873z",
    ] {
        assert_eq!(
            get_configuration(id).await.expect_err("invalid ID").status,
            400
        );
        assert_eq!(
            update_configuration(id, PatchCronConfiguration::default())
                .await
                .expect_err("invalid ID")
                .status,
            400
        );
        assert_eq!(
            delete_configuration(id)
                .await
                .expect_err("invalid ID")
                .status,
            400
        );
    }
}
