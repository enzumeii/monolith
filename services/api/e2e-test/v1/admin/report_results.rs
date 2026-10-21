#![allow(clippy::unwrap_used)]
use crate::common::{TestClient, TestFixture, authed_json, body_json, client, fixture};
use domain::dto::AdminRole;
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn accepted_mail_is_not_repeated_when_whole_job_retries_publication() {
    let fixture = fixture().await;
    let (url, upstream) = inventory_upstream().await;
    let mail = ReportMail::start("block-storage").await;
    let client = worker(&url, &mail.url, 10).await;
    *mail.storage.lock().unwrap() = Some(client.reports_directory());
    let config = queue(&fixture, &client, false).await;
    let failed = wait(&fixture, &client, config, "FAILED").await;
    let id = Uuid::parse_str(failed["id"].as_str().unwrap()).unwrap();
    assert!(failed["error"].as_str().unwrap().contains("publication"));
    assert!(failed["generated_at"].is_string());
    assert_eq!(failed["files"].as_array().unwrap().len(), 0);
    assert_eq!(mail.requests.lock().unwrap().len(), 1);
    assert_eq!(attempts(&fixture, id).await, 1);
    tokio::fs::remove_file(
        client
            .reports_directory()
            .join("public")
            .join(id.to_string()),
    )
    .await
    .unwrap();
    make_due(&fixture, id).await;
    let success = wait(&fixture, &client, config, "SUCCESS").await;
    assert_eq!(success["generated_at"], failed["generated_at"]);
    assert_eq!(attempts(&fixture, id).await, 2);
    let reference = &success["files"][0];
    let bytes = reqwest::get(reference["download_url"].as_str().unwrap())
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    {
        let requests = mail.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(requests[0].1["attachments"][0]["content"].clone())
                .unwrap(),
            bytes
        );
    }
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .starts_with("category,collection,sku,")
    );
    assert_eq!(
        reqwest::get(format!("{}.pending/{id}/email.json", client.storage_base()))
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/cron/reports/{id}/download"),
            fixture.root_id()
        )
        .await
        .status(),
        404
    );
    let deleted = client
        .send(authed_json(
            "DELETE",
            &format!("/v1/admin/cron/configs/{config}"),
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
            &format!("/v1/admin/cron/reports/{id}"),
            fixture.root_id()
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        reqwest::get(reference["download_url"].as_str().unwrap())
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(upstream.requests().len(), 4);
}
#[tokio::test]
async fn email_failure_is_eager_and_retry_reuses_the_frozen_request_and_files() {
    let fixture = fixture().await;
    let (url, upstream) = inventory_upstream().await;
    let mail = ReportMail::start("reject-first").await;
    let client = worker(&url, &mail.url, 10).await;
    let config = queue(&fixture, &client, false).await;
    let failed = wait(&fixture, &client, config, "FAILED").await;
    assert!(failed["generated_at"].is_string());
    assert!(failed["error"].as_str().unwrap().contains("Email"));
    let id = Uuid::parse_str(failed["id"].as_str().unwrap()).unwrap();
    let patched = client
        .send(authed_json(
            "PATCH",
            &format!("/v1/admin/cron/configs/{config}"),
            fixture.root_id(),
            AdminRole::SuperAdmin,
            &json!({"receiver_email":"changed@example.test","parameter":"999"}),
        ))
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);
    make_due(&fixture, id).await;
    let success = wait(&fixture, &client, config, "SUCCESS").await;
    assert_eq!(success["parameters"], "12345");
    assert_eq!(success["generated_at"], failed["generated_at"]);
    assert!(success["error"].is_null());
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].1, requests[1].1);
    assert_eq!(
        requests[0].0["idempotency-key"],
        requests[1].0["idempotency-key"]
    );
    assert_eq!(requests[0].1["to"][0], "reports@example.test");
    assert_eq!(upstream.requests().len(), 4);
}
#[tokio::test]
async fn slow_email_keeps_its_lease_and_finishes_one_attempt() {
    let fixture = fixture().await;
    let (url, _upstream) = inventory_upstream().await;
    let mail = ReportMail::start("slow").await;
    let client = worker(&url, &mail.url, 10).await;
    let config = queue(&fixture, &client, false).await;
    let audit = wait(&fixture, &client, config, "SUCCESS").await;
    assert_eq!(mail.requests.lock().unwrap().len(), 1);
    assert_eq!(
        attempts(
            &fixture,
            Uuid::parse_str(audit["id"].as_str().unwrap()).unwrap()
        )
        .await,
        1
    );
}
#[tokio::test]
async fn dispatch_without_next_run_persists_generation_failure_and_delete_cancels_jobs() {
    let fixture = fixture().await;
    let client =
        crate::common::client_with_scheduler(crate::common::ProviderConfig::default(), 60).await;
    let config = queue(&fixture, &client, true).await;
    let audit = wait(&fixture, &client, config, "FAILED").await;
    assert!(audit["generated_at"].is_null());
    assert_eq!(audit["error"], "Report generation failed");
    let id = Uuid::parse_str(audit["id"].as_str().unwrap()).unwrap();
    client
        .send(authed_json(
            "DELETE",
            &format!("/v1/admin/cron/configs/{config}"),
            fixture.root_id(),
            AdminRole::SuperAdmin,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cron_jobs WHERE cron_configuration_id=$1"
        )
        .bind(config)
        .fetch_one(&fixture.pool())
        .await
        .unwrap(),
        0
    );
    let audit = body_json(
        get(
            &client,
            &format!("/v1/admin/cron/reports/{id}"),
            fixture.root_id(),
        )
        .await,
    )
    .await;
    assert_eq!(audit["status"], "FAILED");
    assert!(
        audit["error"]
            .as_str()
            .unwrap()
            .contains("Schedule deleted")
    );
}
#[tokio::test]
async fn history_has_one_status_and_direct_urls_for_multiple_saved_csv_files() {
    let fixture = fixture().await;
    let client = client().await;
    let id = seed(&fixture, &client).await;
    let detail = body_json(
        get(
            &client,
            &format!("/v1/admin/cron/reports/{id}"),
            fixture.root_id(),
        )
        .await,
    )
    .await;
    assert_eq!(detail["status"], "SUCCESS");
    for removed in [
        "generation_status",
        "email_status",
        "generation_error",
        "email_error",
        "cron_name",
        "period_start",
        "inventory_id",
    ] {
        assert!(detail.get(removed).is_none());
    }
    for file in detail["files"].as_array().unwrap() {
        assert!(file.get("storage_key").is_none());
        let response = reqwest::get(file["download_url"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .contains("attachment")
        );
        assert!(response.text().await.unwrap().contains("original"));
    }
    let listed=body_json(get(&client,"/v1/admin/cron/reports?report_type=TRANSACTION&generated_from=2026-10-01T00%3A00%3A00%2B07%3A00&generated_to=2026-11-02T00%3A00%3A00%2B07%3A00&per_page=1",fixture.root_id()).await).await;
    assert_eq!(listed["items"][0]["id"], id.to_string());
    assert_eq!(listed["has_next"], false);
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/cron/reports/{id}/download"),
            fixture.root_id()
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        reqwest::get(format!("{}{id}/unknown.csv", client.storage_base()))
            .await
            .unwrap()
            .status(),
        404
    );
}
#[tokio::test]
async fn history_query_validation_covers_invalid_dates_bounds_and_legacy_fields() {
    let fixture = fixture().await;
    let client = client().await;
    for query in [
        "page=0",
        "per_page=0",
        "per_page=101",
        "report_type=UNKNOWN",
        "generated_from=invalid",
        "generated_to=2026-10-01T00:00:00",
        "generated_from=2026-11-01T00:00:00Z&generated_to=2026-10-01T00:00:00Z",
        "configuration_id=anything",
        "generation_status=FAILED",
        "scheduled_from=2026-10-01",
        "unknown=1",
    ] {
        assert_eq!(
            get(
                &client,
                &format!("/v1/admin/cron/reports?{query}"),
                fixture.root_id()
            )
            .await
            .status(),
            400,
            "{query}"
        );
    }
    assert_eq!(
        get(
            &client,
            "/v1/admin/cron/reports/not-a-uuid",
            fixture.root_id()
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        get(
            &client,
            &format!("/v1/admin/cron/reports/{}", Uuid::new_v4()),
            fixture.root_id()
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        get(&client, "/v1/admin/report-results", fixture.root_id())
            .await
            .status(),
        404
    );
}

async fn inventory_upstream() -> (String, crate::common::ScriptedUpstream) {
    crate::common::scripted_upstream(
        vec![
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],Some("products-2"))}),
            json!({"location":{"id":"gid://shopify/Location/12345"}}),
            json!({"products":crate::common::shopify_page(vec![],None)}),
        ],
        "http",
    )
    .await
}
async fn worker(url: &str, mail: &str, timeout: u64) -> TestClient {
    crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(url),
            resend_url: Some(mail),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(timeout),
            ..Default::default()
        },
        60,
    )
    .await
}
async fn queue(fixture: &TestFixture, client: &TestClient, dispatch: bool) -> Uuid {
    let response=client.send(authed_json("POST","/v1/admin/cron/configs",fixture.root_id(),AdminRole::SuperAdmin,&json!({
        "name":format!("{}-pipeline",fixture.prefix()),"report_type":"INVENTORY","cron_expression":"* * * * *",
        "receiver_email":"reports@example.test","parameter":"12345","enabled":dispatch
    }))).await.unwrap();
    assert_eq!(response.status(), 201);
    let config = Uuid::parse_str(body_json(response).await["id"].as_str().unwrap()).unwrap();
    if dispatch {
        sqlx::query("UPDATE cron_configurations SET updated_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '61 seconds' WHERE id=$1").bind(config).execute(&fixture.pool()).await.unwrap();
    } else {
        sqlx::query("INSERT INTO cron_jobs (cron_configuration_id,scheduled_for) VALUES ($1,CURRENT_TIMESTAMP AT TIME ZONE 'UTC')").bind(config).execute(&fixture.pool()).await.unwrap();
    }
    config
}
async fn wait(fixture: &TestFixture, client: &TestClient, config: Uuid, status: &str) -> Value {
    for _ in 0..400 {
        let id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM cron_jobs WHERE cron_configuration_id=$1 ORDER BY scheduled_for LIMIT 1").bind(config).fetch_optional(&fixture.pool()).await.unwrap();
        if let Some(id) = id {
            fixture.own_report(id);
            let state: Option<String> =
                sqlx::query_scalar("SELECT status FROM report_results WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&fixture.pool())
                    .await
                    .unwrap();
            if state.as_deref() == Some(status) {
                return body_json(
                    get(
                        client,
                        &format!("/v1/admin/cron/reports/{id}"),
                        fixture.root_id(),
                    )
                    .await,
                )
                .await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("audit did not reach {status}: {}", client.logs());
}
async fn attempts(fixture: &TestFixture, id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT attempts FROM cron_jobs WHERE id=$1")
        .bind(id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap()
}
async fn make_due(fixture: &TestFixture, id: Uuid) {
    sqlx::query("UPDATE cron_jobs SET lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second' WHERE id=$1").bind(id).execute(&fixture.pool()).await.unwrap();
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
async fn seed(fixture: &TestFixture, client: &TestClient) -> Uuid {
    let id = Uuid::new_v4();
    fixture.own_report(id);
    let directory = client
        .reports_directory()
        .join("public")
        .join(id.to_string());
    tokio::fs::create_dir_all(&directory).await.unwrap();
    let mut files = Vec::new();
    for name in ["transactions.csv", "items.csv"] {
        tokio::fs::write(directory.join(name), b"header\r\noriginal\r\n")
            .await
            .unwrap();
        files.push(json!({"filename":name,"content_type":"text/csv; charset=utf-8","storage_key":format!("{id}/{name}"),"download_url":format!("{}{id}/{name}",client.storage_base())}));
    }
    sqlx::query("INSERT INTO report_results (id,report_type,attempted_at,generated_at,status,parameters,files) VALUES ($1,'TRANSACTION','2026-11-01 00:00:00','2026-11-01 00:00:00','SUCCESS',$2,$3)")
        .bind(id).bind("2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00").bind(json!(files)).execute(&fixture.pool()).await.unwrap();
    id
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
            std::sync::Arc::default();
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
                    std::fs::create_dir_all(root.join("public")).unwrap();
                    std::fs::write(root.join("public").join(id),b"block destination").unwrap();
                }
                recorded.lock().unwrap().push((headers,body));
                if mode == "slow" { tokio::time::sleep(std::time::Duration::from_secs(6)).await; }
                if mode == "reject" || (mode == "reject-first" && recorded.lock().unwrap().len()==1) {
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
