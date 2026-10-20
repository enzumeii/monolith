#![allow(clippy::unwrap_used)]

use crate::common::{authed_json, body_json, client, fixture, seed_admin};
use domain::dto::AdminRole;
use serde_json::json;

#[tokio::test]
async fn admin_can_create_a_disabled_transaction_schedule_over_tcp() {
    let fixture = fixture().await;
    let admin_id = seed_admin(
        &fixture,
        fixture.username("ordinary-admin"),
        "fixture-password",
        AdminRole::Admin,
    )
    .await;
    let name = format!("{}-transaction", fixture.prefix());
    let client = client().await;
    let response = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron-configurations",
            admin_id,
            AdminRole::Admin,
            &json!({
                "name": name,
                "report_type": "TRANSACTION",
                "cron_expression": "0 0 1 * *",
                "receiver_email": "reports@example.test",
                "period_mode": "PREVIOUS_MONTH",
                "inventory_id": null,
                "status": "DISABLED"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .starts_with("/v1/admin/cron-configurations/")
    );
    let body = body_json(response).await;
    assert_eq!(body["name"], name);
    assert_eq!(body["report_type"], "TRANSACTION");
    assert_eq!(body["schedule_timezone"], "Asia/Bangkok");
    assert_eq!(body["status"], "DISABLED");
    assert!(body["next_run_at"].is_null());

    let stored_name: String =
        sqlx::query_scalar("SELECT name FROM cron_configurations WHERE id=$1::uuid")
            .bind(body["id"].as_str().unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(stored_name, name);
    sqlx::query("DELETE FROM cron_configurations WHERE name=$1")
        .bind(name)
        .execute(&fixture.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn admin_can_preview_list_patch_and_delete_schedules_over_tcp() {
    let fixture = fixture().await;
    let admin_id = seed_admin(
        &fixture,
        fixture.username("schedule-crud-admin"),
        "fixture-password",
        AdminRole::Admin,
    )
    .await;
    let client = client().await;
    let preview = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron-configurations/preview",
            admin_id,
            AdminRole::Admin,
            &json!({
                "report_type": "TRANSACTION",
                "cron_expression": "0 9 1 * *",
                "period_mode": "PREVIOUS_MONTH"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    let preview = body_json(preview).await;
    assert_eq!(preview["schedule_timezone"], "Asia/Bangkok");
    assert_eq!(preview["occurrences"].as_array().unwrap().len(), 5);

    let name = format!("{}-crud", fixture.prefix());
    let created = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron-configurations",
            admin_id,
            AdminRole::Admin,
            &json!({
                "name": name,
                "report_type": "TRANSACTION",
                "cron_expression": "0 9 1 * *",
                "receiver_email": "reports@example.test",
                "period_mode": "PREVIOUS_MONTH",
                "status": "DISABLED"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let created = body_json(created).await;
    let id = created["id"].as_str().unwrap();

    let listed = client
        .send(authed_json(
            "GET",
            "/v1/admin/cron-configurations?page=1&per_page=1",
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    assert!(
        body_json(listed).await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == id)
    );
    let super_admin_list = client
        .send(authed_json(
            "GET",
            "/v1/admin/cron-configurations?page=1&per_page=1",
            fixture.root_id(),
            AdminRole::SuperAdmin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(super_admin_list.status(), 200);

    let patched = client
        .send(authed_json(
            "PATCH",
            &format!("/v1/admin/cron-configurations/{id}"),
            admin_id,
            AdminRole::Admin,
            &json!({"name":"renamed schedule"}),
        ))
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);
    assert_eq!(body_json(patched).await["name"], "renamed schedule");

    let deleted = client
        .send(authed_json(
            "DELETE",
            &format!("/v1/admin/cron-configurations/{id}"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    let missing = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/cron-configurations/{id}"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}
