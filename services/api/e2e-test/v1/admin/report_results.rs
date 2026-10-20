#![allow(clippy::unwrap_used)]

use crate::common::{authed_json, body_json, client, fixture, seed_admin};
use axum::http::Method;
use domain::dto::AdminRole;
use http_body_util::BodyExt;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn admin_can_read_rows_and_download_only_the_saved_report_over_tcp() {
    let fixture = fixture().await;
    let admin_id = seed_admin(
        &fixture,
        fixture.username("saved-report-admin"),
        "fixture-password",
        AdminRole::Admin,
    )
    .await;
    let client = client().await;
    let config_name = format!("{}-saved", fixture.prefix());
    let created = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron-configurations",
            admin_id,
            AdminRole::Admin,
            &json!({
                "name": config_name,
                "report_type": "INVENTORY",
                "cron_expression": "0 9 * * *",
                "receiver_email": "reports@example.test",
                "inventory_id": "12345",
                "status": "DISABLED"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let configuration_id =
        Uuid::parse_str(body_json(created).await["id"].as_str().unwrap()).unwrap();
    let report_id = Uuid::new_v4();
    let report_data = json!({
        "inventory": [{
            "category": "Tools",
            "collection": "Spring",
            "sku": "=HYPERLINK(\"https://example.test\")",
            "title": "Hammer",
            "variant": "Blue",
            "cost": 2.5,
            "cost_currency": "THB",
            "weight": 1,
            "unit": "kg",
            "selling_price": 4.5,
            "selling_price_currency": "THB",
            "quantity": 3,
            "total_selling_price": 13.5,
            "total_weight": 3
        }]
    });
    sqlx::query("INSERT INTO report_results (id, cron_configuration_id, configuration_id_snapshot, cron_name, cron_expression, report_type, receiver_email, inventory_id, scheduled_for, generation_status, email_status, report_data, row_count, started_at, generated_at) VALUES ($1,$2,$2,$3,'0 9 * * *','INVENTORY','reports@example.test','12345',CURRENT_TIMESTAMP AT TIME ZONE 'UTC','SUCCEEDED','PENDING',$4,1,CURRENT_TIMESTAMP AT TIME ZONE 'UTC',CURRENT_TIMESTAMP AT TIME ZONE 'UTC')")
        .bind(report_id)
        .bind(configuration_id)
        .bind(&config_name)
        .bind(report_data)
        .execute(&fixture.pool())
        .await
        .unwrap();

    let list = client
        .send(authed_json(
            "GET",
            "/v1/admin/report-results",
            admin_id,
            AdminRole::Admin,
            &json!({"per_page":25}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    assert!(
        body_json(list).await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == report_id.to_string())
    );

    let detail = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/report-results/{report_id}"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(detail.status(), 200);
    assert_eq!(
        body_json(detail).await["available_datasets"][0],
        "inventory"
    );

    let rows = client
        .send(authed_json(
            "GET",
            &format!(
                "/v1/admin/report-results/{report_id}/rows?dataset=inventory&page=1&per_page=10"
            ),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(rows.status(), 200);
    assert_eq!(
        body_json(rows).await["items"][0]["sku"],
        "=HYPERLINK(\"https://example.test\")"
    );

    let download = client
        .send(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/admin/report-results/{report_id}/download?dataset=inventory"
                ))
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        crate::common::jwt_for(admin_id, AdminRole::Admin)
                    ),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), 200);
    let disposition = download.headers()["content-disposition"].to_str().unwrap();
    assert!(disposition.contains("inventory.csv"));
    assert!(!disposition.contains('/'));
    let csv = String::from_utf8(
        download
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(csv.contains("'=HYPERLINK"));
    assert!(csv.ends_with("\r\n"));

    let not_ready_id = Uuid::new_v4();
    sqlx::query("INSERT INTO report_results (id, cron_configuration_id, configuration_id_snapshot, cron_name, cron_expression, report_type, receiver_email, inventory_id, scheduled_for, generation_status, email_status) VALUES ($1,$2,$2,$3,'0 9 * * *','INVENTORY','reports@example.test','12345',CURRENT_TIMESTAMP AT TIME ZONE 'UTC','QUEUED','NOT_READY')")
        .bind(not_ready_id)
        .bind(configuration_id)
        .bind(&config_name)
        .execute(&fixture.pool())
        .await
        .unwrap();
    let not_ready = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/report-results/{not_ready_id}/download?dataset=inventory"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(not_ready.status(), 409);

    let wrong_dataset = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/report-results/{report_id}/rows?dataset=transactions"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(wrong_dataset.status(), 400);
    let invalid_page = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/report-results/{report_id}/rows?dataset=inventory&per_page=0"),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_page.status(), 400);
    let invalid_id = client
        .send(authed_json(
            "GET",
            "/v1/admin/report-results/not-a-uuid",
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_id.status(), 400);
    let missing_id = client
        .send(authed_json(
            "GET",
            &format!("/v1/admin/report-results/{}", Uuid::new_v4()),
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(missing_id.status(), 404);

    sqlx::query("DELETE FROM report_results WHERE id=$1")
        .bind(report_id)
        .execute(&fixture.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM report_results WHERE id=$1")
        .bind(not_ready_id)
        .execute(&fixture.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM cron_configurations WHERE id=$1")
        .bind(configuration_id)
        .execute(&fixture.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn scheduler_dispatches_due_inventory_and_persists_provider_failure() {
    let fixture = fixture().await;
    let admin_id = seed_admin(
        &fixture,
        fixture.username("scheduler-admin"),
        "fixture-password",
        AdminRole::Admin,
    )
    .await;
    let client = client().await;
    let name = format!("{}-worker", fixture.prefix());
    let created = client
        .send(authed_json(
            "POST",
            "/v1/admin/cron-configurations",
            admin_id,
            AdminRole::Admin,
            &json!({
                "name": name,
                "report_type": "INVENTORY",
                "cron_expression": "0 9 * * *",
                "receiver_email": "reports@example.test",
                "inventory_id": "12345",
                "status": "ENABLED"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let config_id = Uuid::parse_str(body_json(created).await["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE cron_configurations SET next_run_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC') - INTERVAL '1 minute' WHERE id=$1")
        .bind(config_id)
        .execute(&fixture.pool())
        .await
        .unwrap();

    let mut status = None;
    for _ in 0..80 {
        status = sqlx::query_scalar::<_, String>("SELECT generation_status FROM report_results WHERE configuration_id_snapshot=$1 ORDER BY scheduled_for DESC LIMIT 1")
            .bind(config_id)
            .fetch_optional(&fixture.pool())
            .await
            .unwrap();
        if status.as_deref() == Some("FAILED") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert_eq!(status.as_deref(), Some("FAILED"));
    let next_run: chrono::NaiveDateTime =
        sqlx::query_scalar("SELECT next_run_at FROM cron_configurations WHERE id=$1")
            .bind(config_id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert!(next_run > chrono::Utc::now().naive_utc());
}
