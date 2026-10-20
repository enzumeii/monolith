#![allow(clippy::unwrap_used)]
use crate::common::{TestClient, TestFixture, authed_json, body_json, client, fixture, seed_admin};
use domain::dto::AdminRole;
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[tokio::test]
async fn accepted_email_is_not_repeated_when_file_publication_retries() {
    let fixture = fixture().await;
    let (url, upstream) = crate::common::scripted_upstream(
        vec![
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],Some("products-2"))}),
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],None)}),
        ],
        "http",
    )
    .await;
    let mail = ReportMail::start("block-storage").await;
    let client = crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(&url),
            resend_url: Some(&mail.url),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(10),
            ..Default::default()
        },
        60,
    )
    .await;
    *mail.storage.lock().unwrap() = Some(client.reports_directory());
    let configuration = queue_inventory(&fixture, &client, "CSV").await;
    let audit = wait_audit(&fixture, configuration, "SENT").await;
    let id = Uuid::parse_str(audit["id"].as_str().unwrap()).unwrap();
    fixture.own_report(id);
    assert!(audit["files"].as_array().unwrap().is_empty());
    let mut failed = false;
    for _ in 0..100 {
        failed = sqlx::query_scalar::<_, Option<String>>(
            "SELECT publication_error FROM cron_jobs WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap()
        .is_some();
        if failed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(failed);
    assert_eq!(mail.requests.lock().unwrap().len(), 1);
    let attempts: i32 =
        sqlx::query_scalar("SELECT publication_attempts FROM cron_jobs WHERE id=$1")
            .bind(id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(
        attempts, 1,
        "fixture must retain its second publication attempt"
    );
    std::fs::remove_file(client.reports_directory().join(id.to_string())).unwrap();
    sqlx::query(
        "UPDATE cron_jobs SET next_attempt_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC') WHERE id=$1",
    )
    .bind(id)
    .execute(&fixture.pool())
    .await
    .unwrap();
    let mut published = None;
    for _ in 0..100 {
        let body = body_json(
            get(
                &client,
                &format!("/v1/admin/report-results/{id}"),
                fixture.root_id(),
            )
            .await,
        )
        .await;
        if body["files"].as_array().unwrap().len() == 1 {
            published = Some(body);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let published =
        published.unwrap_or_else(|| panic!("publication retry did not finish: {}", client.logs()));
    let download = get(
        &client,
        published["files"][0]["download_url"].as_str().unwrap(),
        fixture.root_id(),
    )
    .await;
    assert_eq!(download.status(), 200);
    let bytes = download.into_body().collect().await.unwrap().to_bytes();
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(requests[0].1["attachments"][0]["content"].clone())
            .unwrap(),
        bytes
    );
    assert_eq!(
        requests[0].0["idempotency-key"],
        format!("report-result-{id}")
    );
    assert_eq!(upstream.requests().len(), 4);
    assert_eq!(upstream.requests()[3]["variables"]["after"], "products-2");
    drop(requests);
    let deleted = client
        .send(authed_json(
            "DELETE",
            &format!("/v1/admin/cron-configurations/{configuration}"),
            fixture.root_id(),
            AdminRole::SuperAdmin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204);
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{id}/download"),
            fixture.root_id()
        )
        .await
        .status(),
        200
    );
}

#[tokio::test]
async fn failed_email_is_eagerly_audited_and_reuses_the_generated_files() {
    let fixture = fixture().await;
    let (url, upstream) = crate::common::scripted_upstream(
        vec![
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],None)}),
        ],
        "http",
    )
    .await;
    let mail = ReportMail::start("reject").await;
    let client = crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(&url),
            resend_url: Some(&mail.url),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(10),
            ..Default::default()
        },
        1,
    )
    .await;
    let configuration = queue_inventory(&fixture, &client, "CSV").await;
    let audit = wait_audit(&fixture, configuration, "FAILED").await;
    assert_eq!(audit["generation_status"], "SUCCEEDED");
    assert!(audit["generated_at"].is_string());
    assert_eq!(audit["email_error"], "Email delivery attempt failed");
    assert!(audit["files"].as_array().unwrap().is_empty());
    let id = Uuid::parse_str(audit["id"].as_str().unwrap()).unwrap();
    fixture.own_report(id);
    assert!(
        client
            .reports_directory()
            .join(".pending")
            .join(id.to_string())
            .join("inventory.csv")
            .is_file()
    );
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{id}/download"),
            fixture.root_id()
        )
        .await
        .status(),
        409
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].1, requests[1].1);
    assert_eq!(
        requests[0].0["idempotency-key"],
        requests[1].0["idempotency-key"]
    );
    assert_eq!(upstream.requests().len(), 2);
}

#[tokio::test]
async fn xlsx_attachments_are_saved_unchanged_and_active_email_leases_are_renewed() {
    let fixture = fixture().await;
    let (url, upstream) = crate::common::scripted_upstream(
        vec![
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],None)}),
        ],
        "http",
    )
    .await;
    let mail = ReportMail::start("slow").await;
    let client = crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(&url),
            resend_url: Some(&mail.url),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(10),
            ..Default::default()
        },
        1,
    )
    .await;
    let configuration = queue_inventory(&fixture, &client, "XLSX").await;
    let mut before = None;
    for _ in 0..100 {
        if !mail.requests.lock().unwrap().is_empty() {
            before = sqlx::query_scalar::<_, Option<chrono::NaiveDateTime>>(
                "SELECT lease_until FROM cron_jobs WHERE configuration_id_snapshot=$1",
            )
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let before = before.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let after: chrono::NaiveDateTime =
        sqlx::query_scalar("SELECT lease_until FROM cron_jobs WHERE configuration_id_snapshot=$1")
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert!(after > before);
    let audit = wait_audit(&fixture, configuration, "SENT").await;
    let id = Uuid::parse_str(audit["id"].as_str().unwrap()).unwrap();
    fixture.own_report(id);
    let mut downloaded = None;
    for _ in 0..100 {
        let result = get(
            &client,
            &format!("/v1/admin/report-results/{id}/download"),
            fixture.root_id(),
        )
        .await;
        if result.status() == 200 {
            downloaded = Some(result);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let downloaded = downloaded.unwrap();
    assert_eq!(
        downloaded.headers()["content-type"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let bytes = downloaded.into_body().collect().await.unwrap().to_bytes();
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].1["attachments"][0]["filename"],
        "inventory.xlsx"
    );
    assert_eq!(
        requests[0].1["attachments"][0]["content_type"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(requests[0].1["attachments"][0]["content"].clone())
            .unwrap(),
        bytes
    );
    assert_eq!(upstream.requests().len(), 2);
}

async fn queue_inventory(fixture: &TestFixture, client: &TestClient, format: &str) -> Uuid {
    let created = client.send(authed_json("POST","/v1/admin/cron-configurations",fixture.root_id(),AdminRole::SuperAdmin,&json!({
        "name":format!("{}-pipeline",fixture.prefix()),"report_type":"INVENTORY","cron_expression":"0 9 * * *",
        "receiver_email":"reports@example.test","inventory_id":"12345","status":"ENABLED","file_format":format
    }))).await.unwrap();
    assert_eq!(created.status(), 201);
    let created = body_json(created).await;
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE cron_configurations SET next_run_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC') - INTERVAL '1 minute' WHERE id=$1")
        .bind(id).execute(&fixture.pool()).await.unwrap();
    id
}

async fn wait_audit(
    fixture: &TestFixture,
    configuration: Uuid,
    email_status: &str,
) -> serde_json::Value {
    for _ in 0..300 {
        let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM cron_jobs WHERE configuration_id_snapshot=$1 ORDER BY scheduled_for DESC LIMIT 1")
            .bind(configuration).fetch_optional(&fixture.pool()).await.unwrap();
        if let Some(id) = id {
            let state: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT email_status,email_error FROM report_results WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&fixture.pool())
                    .await
                    .unwrap();
            if state
                .as_ref()
                .is_some_and(|(status, _)| status == email_status)
            {
                let client = client().await;
                return body_json(
                    get(
                        &client,
                        &format!("/v1/admin/report-results/{id}"),
                        fixture.root_id(),
                    )
                    .await,
                )
                .await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("audit never reached {email_status}");
}

struct ReportMail {
    url: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<(axum::http::HeaderMap, serde_json::Value)>>>,
    storage: std::sync::Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    task: tokio::task::JoinHandle<()>,
}
impl ReportMail {
    async fn start(mode: &'static str) -> Self {
        use axum::{Json, Router, response::IntoResponse, routing::post};
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let storage: std::sync::Arc<std::sync::Mutex<Option<std::path::PathBuf>>> =
            Default::default();
        let recorded = requests.clone();
        let directory = storage.clone();
        let router = Router::new().route("/emails",post(move |headers:axum::http::HeaderMap,Json(body):Json<serde_json::Value>| {
            let recorded = recorded.clone();
            let directory = directory.clone();
            async move {
                assert_eq!(headers["authorization"],"Bearer re_e2e-fixture");
                if mode == "block-storage" {
                    let root = directory.lock().unwrap().clone().unwrap();
                    std::fs::create_dir_all(&root).unwrap();
                    let id = headers["idempotency-key"].to_str().unwrap().strip_prefix("report-result-").unwrap();
                    std::fs::write(root.join(id),b"block destination").unwrap();
                }
                recorded.lock().unwrap().push((headers,body));
                if mode == "slow" { tokio::time::sleep(std::time::Duration::from_secs(6)).await; }
                if mode == "reject" {
                    return (axum::http::StatusCode::SERVICE_UNAVAILABLE,Json(json!({"statusCode":503,"name":"application_error","message":"private recipient fixture"}))).into_response();
                }
                Json(json!({"id":"scheduled-fixture-email"})).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            url,
            requests,
            storage,
            task,
        }
    }
}
impl Drop for ReportMail {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn history_serves_only_saved_files_and_survives_configuration_deletion() {
    let fixture = fixture().await;
    let client = client().await;
    let admin_id = fixture.root_id();
    let id = seed_saved_report(
        &fixture,
        &client,
        "2026-10-01T00:00:00",
        &[(
            "inventory.csv",
            "text/csv; charset=utf-8",
            b"sku,title\r\n1,Original\r\n",
        )],
    )
    .await;
    let detail = get(&client, &format!("/v1/admin/report-results/{id}"), admin_id).await;
    assert_eq!(detail.status(), 200);
    let detail = body_json(detail).await;
    assert_eq!(detail["generation_status"], "SUCCEEDED");
    assert_eq!(detail["email_status"], "SENT");
    assert!(detail.get("configuration_id_snapshot").is_none());
    assert!(detail.get("cron_name").is_none());
    assert!(detail["files"][0].get("storage_key").is_none());
    let url = detail["files"][0]["download_url"].as_str().unwrap();
    let download = get(&client, url, admin_id).await;
    assert_eq!(download.status(), 200);
    assert_eq!(
        download.headers()["content-type"],
        "text/csv; charset=utf-8"
    );
    assert_eq!(download.headers()["cache-control"], "no-store");
    assert_eq!(
        download.headers()["content-disposition"],
        "attachment; filename=\"inventory.csv\""
    );
    assert_eq!(
        download.into_body().collect().await.unwrap().to_bytes(),
        &b"sku,title\r\n1,Original\r\n"[..]
    );
    let default = get(
        &client,
        &format!("/v1/admin/report-results/{id}/download"),
        admin_id,
    )
    .await;
    assert_eq!(default.status(), 200);
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{id}/download?file=..%2Fsecret"),
            admin_id
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{id}/rows?dataset=inventory"),
            admin_id
        )
        .await
        .status(),
        404
    );
    std::fs::remove_file(
        client
            .reports_directory()
            .join(id.to_string())
            .join("inventory.csv"),
    )
    .unwrap();
    let missing = get(&client, url, admin_id).await;
    assert_eq!(missing.status(), 500);
    assert_eq!(
        body_json(missing).await["error"],
        "Saved report file is unavailable"
    );
}

#[tokio::test]
async fn history_filters_validate_dates_bounds_unknown_fields_and_pagination() {
    let fixture = fixture().await;
    let client = client().await;
    let admin = fixture.root_id();
    let first = seed_saved_report(
        &fixture,
        &client,
        "2091-01-01T00:00:00",
        &[("inventory.csv", "text/csv; charset=utf-8", b"first")],
    )
    .await;
    let last = seed_saved_report(
        &fixture,
        &client,
        "2091-01-02T00:00:00",
        &[("inventory.csv", "text/csv; charset=utf-8", b"last")],
    )
    .await;
    let page = get(&client,"/v1/admin/report-results?generated_from=2091-01-01T00:00:00Z&generated_to=2091-01-03T00:00:00Z&per_page=1",admin).await;
    assert_eq!(page.status(), 200);
    let page = body_json(page).await;
    assert_eq!(page["items"][0]["id"], last.to_string());
    assert_eq!(page["has_next"], true);
    let exclusive = body_json(get(&client,"/v1/admin/report-results?generated_from=2091-01-01T00:00:00Z&generated_to=2091-01-02T00:00:00Z",admin).await).await;
    assert_eq!(exclusive["items"].as_array().unwrap().len(), 1);
    assert_eq!(exclusive["items"][0]["id"], first.to_string());
    for query in [
        "page=0",
        "page=-1",
        "page=x",
        "per_page=0",
        "per_page=101",
        "report_type=OTHER",
        "configuration_id=ignored",
        "generation_status=FAILED",
        "scheduled_from=2091-01-01T00:00:00Z",
        "generated_from=2091-01-01",
        "generated_to=nonsense",
        "generated_from=2091-01-02T00:00:00Z&generated_to=2091-01-01T00:00:00Z",
    ] {
        assert_eq!(
            get(&client, &format!("/v1/admin/report-results?{query}"), admin)
                .await
                .status(),
            400,
            "{query}"
        );
    }
    for suffix in ["not-a-uuid", "not-a-uuid/download"] {
        assert_eq!(
            get(
                &client,
                &format!("/v1/admin/report-results/{suffix}"),
                admin
            )
            .await
            .status(),
            400
        );
    }
    let missing = Uuid::new_v4();
    for suffix in [missing.to_string(), format!("{missing}/download")] {
        assert_eq!(
            get(
                &client,
                &format!("/v1/admin/report-results/{suffix}"),
                admin
            )
            .await
            .status(),
            404
        );
    }
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{first}/download?dataset=inventory"),
            admin
        )
        .await
        .status(),
        400
    );
    for path in [
        "/v1/admin/report-results".to_owned(),
        format!("/v1/admin/report-results/{first}"),
        format!("/v1/admin/report-results/{first}/download"),
    ] {
        crate::common::assert_report_auth(&client, &path).await;
    }
    sqlx::query("UPDATE report_results SET files = '[\"invalid-reference\"]'::jsonb WHERE id=$1")
        .bind(first)
        .execute(&fixture.pool())
        .await
        .unwrap();
    assert_eq!(
        get(&client, &format!("/v1/admin/report-results/{first}"), admin)
            .await
            .status(),
        500
    );
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/report-results/{first}/download"),
            admin
        )
        .await
        .status(),
        500
    );
}

#[tokio::test]
async fn history_selects_multiple_csv_or_xlsx_files_by_known_name() {
    let fixture = fixture().await;
    let client = client().await;
    let id = seed_saved_report(
        &fixture,
        &client,
        "2026-10-02T00:00:00",
        &[
            (
                "transactions.csv",
                "text/csv; charset=utf-8",
                b"transactions",
            ),
            ("items.csv", "text/csv; charset=utf-8", b"items"),
        ],
    )
    .await;
    let audit = body_json(
        get(
            &client,
            &format!("/v1/admin/report-results/{id}"),
            fixture.root_id(),
        )
        .await,
    )
    .await;
    assert_eq!(audit["report_type"], "TRANSACTION");
    assert!(audit["inventory_id"].is_null());
    let downloaded = get(
        &client,
        &format!("/v1/admin/report-results/{id}/download?file=items.csv"),
        fixture.root_id(),
    )
    .await;
    assert_eq!(
        downloaded.into_body().collect().await.unwrap().to_bytes(),
        &b"items"[..]
    );
    let xlsx = seed_saved_report(
        &fixture,
        &client,
        "2026-10-03T00:00:00",
        &[(
            "inventory.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            b"PK fixture bytes",
        )],
    )
    .await;
    let downloaded = get(
        &client,
        &format!("/v1/admin/report-results/{xlsx}/download"),
        fixture.root_id(),
    )
    .await;
    assert_eq!(downloaded.status(), 200);
    assert_eq!(
        downloaded.headers()["content-type"],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    assert_eq!(
        downloaded.into_body().collect().await.unwrap().to_bytes(),
        &b"PK fixture bytes"[..]
    );
}

async fn get(client: &TestClient, url: &str, admin: i64) -> axum::response::Response {
    client
        .send(authed_json(
            "GET",
            url,
            admin,
            AdminRole::SuperAdmin,
            &json!({}),
        ))
        .await
        .unwrap()
}

async fn seed_saved_report(
    fixture: &TestFixture,
    client: &TestClient,
    timestamp: &str,
    files: &[(&str, &str, &[u8])],
) -> Uuid {
    let id = Uuid::new_v4();
    fixture.own_report(id);
    let directory = client.reports_directory().join(id.to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let entries = files
        .iter()
        .map(|(name, mime, bytes)| {
            std::fs::write(directory.join(name), bytes).unwrap();
            json!({"filename":name,"content_type":mime,"sha256":hex::encode(Sha256::digest(bytes))})
        })
        .collect::<Vec<_>>();
    std::fs::write(directory.join("manifest.json"),serde_json::to_vec(&json!({
        "id":id, "generated_at":timestamp, "file_format":if files[0].0.ends_with(".xlsx") {"XLSX"} else {"CSV"}, "files":entries
    })).unwrap()).unwrap();
    let references = files
        .iter()
        .map(|(name, mime, _)| {
            json!({
                "filename":name,"content_type":mime,"storage_key":format!("{id}/{name}"),
                "download_url":format!("/v1/admin/report-results/{id}/download?file={name}")
            })
        })
        .collect::<Vec<_>>();
    let transaction = files[0].0.starts_with("transactions.");
    sqlx::query("INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status, inventory_id, files) VALUES ($1,$4,$2::text::timestamp,$2::text::timestamp,'SUCCEEDED','SENT',$5,$3)")
        .bind(id).bind(timestamp).bind(json!(references))
        .bind(if transaction { "TRANSACTION" } else { "INVENTORY" })
        .bind(if transaction { None } else { Some("123") })
        .execute(&fixture.pool()).await.unwrap();
    id
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
    let client =
        crate::common::client_with_scheduler(crate::common::ProviderConfig::default(), 1).await;
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
        status = sqlx::query_scalar::<_, String>("SELECT generation_status FROM cron_jobs WHERE configuration_id_snapshot=$1 ORDER BY scheduled_for DESC LIMIT 1")
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
    let failed = client
        .send(authed_json(
            "GET",
            "/v1/admin/report-results?report_type=INVENTORY",
            admin_id,
            AdminRole::Admin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(failed.status(), 200);
    let failed = body_json(failed).await;
    let item = failed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["generation_status"] == "FAILED")
        .unwrap();
    assert!(item["generated_at"].is_null());
    assert_eq!(item["email_status"], "NOT_READY");
    assert_eq!(item["generation_error"], "Report generation failed");
    assert!(item["files"].as_array().unwrap().is_empty());
    let next_run: chrono::NaiveDateTime =
        sqlx::query_scalar("SELECT next_run_at FROM cron_configurations WHERE id=$1")
            .bind(config_id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert!(next_run > chrono::Utc::now().naive_utc());
}
