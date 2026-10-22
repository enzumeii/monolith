#![allow(clippy::unwrap_used)]
use crate::common::{authed_json, body_json, client, fixture, seed_admin};
use domain::dto::AdminRole;
use serde_json::json;

#[tokio::test]
async fn admin_can_create_preview_list_patch_and_delete_new_contract() {
    let fixture = fixture().await;
    let admin = seed_admin(
        &fixture,
        fixture.username("schedule-admin"),
        "fixture-password",
        AdminRole::Admin,
    )
    .await;
    let client = client().await;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cron_configurations")
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
    let preview=client.send(authed_json("GET","/v1/admin/cron/configs/preview?report_type=TRANSACTION&cron_expression=0%209%201%20*%20*&parameter=PREVIOUS_MONTH",admin,AdminRole::Admin,&json!({}))).await.unwrap();
    assert_eq!(preview.status(), 200);
    assert_eq!(
        body_json(preview).await["occurrences"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cron_configurations")
            .fetch_one(&fixture.pool())
            .await
            .unwrap(),
        before
    );
    let created=client.send(authed_json("POST","/v1/admin/cron/configs",admin,AdminRole::Admin,&json!({
        "name":format!("{}-config",fixture.prefix()),"report_type":"TRANSACTION","cron_expression":"0 9 1 * *",
        "receiver_email":"reports@example.test","parameter":"PREVIOUS_MONTH"
    }))).await.unwrap();
    assert_eq!(created.status(), 201);
    assert!(!created.headers().contains_key("location"));
    let created = body_json(created).await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["enabled"], false);
    for removed in [
        "status",
        "next_run_at",
        "period_mode",
        "inventory_id",
        "file_format",
    ] {
        assert!(created.get(removed).is_none());
    }
    let patched=client.send(authed_json("PATCH",&format!("/v1/admin/cron/configs/{id}"),admin,AdminRole::Admin,&json!({"report_type":"INVENTORY","parameter":"123","cron_expression":"0 9 * * *","enabled":true}))).await.unwrap();
    assert_eq!(patched.status(), 200);
    assert_eq!(body_json(patched).await["enabled"], true);
    let listed = client
        .send(authed_json(
            "GET",
            "/v1/admin/cron/configs?enabled=true&report_type=INVENTORY&per_page=1",
            admin,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    assert_eq!(body_json(listed).await["items"][0]["id"], id);
    let got = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/cron/configs/{id}"),
            admin,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    for method in ["DELETE", "GET"] {
        let response = client
            .send(authed_json(
                method,
                &format!("/v1/admin/cron/configs/{id}"),
                admin,
                AdminRole::Admin,
                &json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if method == "DELETE" { 204 } else { 404 }
        );
    }
}
#[tokio::test]
async fn configuration_validation_rejects_each_invalid_input_before_writing() {
    let fixture = fixture().await;
    let client = client().await;
    let valid = json!({"name":format!("{}-validation",fixture.prefix()),"report_type":"INVENTORY","cron_expression":"0 9 * * *","receiver_email":"reports@example.test","parameter":"123"});
    reject_create_fields(&fixture, &client, &valid).await;
    for query in [
        "page=0",
        "per_page=0",
        "per_page=101",
        "enabled=ENABLED",
        "status=ENABLED",
        "unknown=x",
    ] {
        let response = client
            .send(authed_json(
                "GET",
                &format!("/v1/admin/cron/configs?{query}"),
                fixture.root_id(),
                AdminRole::SuperAdmin,
                &json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{query}");
    }
    let response = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron/configs",
            fixture.root_id(),
            AdminRole::SuperAdmin,
            &valid,
        ))
        .await
        .unwrap();
    let id = body_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    for patch in [
        json!({}),
        json!({"enabled":null}),
        json!({"parameter":null}),
        json!({"report_type":"TRANSACTION"}),
        json!({"name":""}),
        json!({"period_mode":"PREVIOUS_MONTH"}),
    ] {
        let response = client
            .send(authed_json(
                "PATCH",
                &format!("/v1/admin/cron/configs/{id}"),
                fixture.root_id(),
                AdminRole::SuperAdmin,
                &patch,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{patch}");
    }
    for path in [
        "/v1/admin/cron/configs/not-a-uuid",
        "/v1/admin/cron/configs/preview?report_type=INVENTORY&cron_expression=bad&parameter=123",
    ] {
        assert_eq!(
            client
                .send(authed_json(
                    "GET",
                    path,
                    fixture.root_id(),
                    AdminRole::SuperAdmin,
                    &json!({})
                ))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(
        client
            .send(authed_json(
                "POST",
                "/v1/admin/cron/configs/preview",
                fixture.root_id(),
                AdminRole::SuperAdmin,
                &valid
            ))
            .await
            .unwrap()
            .status(),
        405
    );
    assert_eq!(
        client
            .send(authed_json(
                "GET",
                "/v1/admin/cron-configurations",
                fixture.root_id(),
                AdminRole::SuperAdmin,
                &json!({})
            ))
            .await
            .unwrap()
            .status(),
        404
    );
}

async fn reject_create_fields(
    fixture: &crate::common::TestFixture,
    client: &crate::common::TestClient,
    valid: &serde_json::Value,
) {
    for (field, value) in [
        ("name", json!("")),
        ("name", json!("x".repeat(121))),
        ("receiver_email", json!("invalid")),
        ("receiver_email", json!("a..b@example.test")),
        ("parameter", json!("0")),
        ("parameter", json!("01")),
        ("parameter", json!("abc")),
        ("cron_expression", json!("0 0 * *")),
        ("cron_expression", json!("bad bad bad bad bad")),
        ("report_type", json!("UNKNOWN")),
        ("enabled", json!("ENABLED")),
        ("file_format", json!("XLSX")),
    ] {
        let mut body = valid.clone();
        body[field] = value;
        let response = client
            .send(authed_json(
                "POST",
                "/v1/admin/cron/configs",
                fixture.root_id(),
                AdminRole::SuperAdmin,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{body}");
    }
}
