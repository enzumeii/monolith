#![allow(clippy::unwrap_used)]
use crate::common::{TestClient, TestFixture, authed_json, body_json, client, fixture};
use domain::dto::AdminRole;
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn cron_consumes_invalid_oldest_job_and_completes_valid_work_in_one_invocation() {
    let fixture = fixture().await;
    let (url, _upstream) = inventory_upstream().await;
    let mail = ReportMail::start("ok").await;
    let client = worker(&url, &mail.url, 10).await;
    let bad = queue(&fixture, &client, false).await;
    sqlx::query("UPDATE cron_jobs SET scheduled_for=scheduled_for-INTERVAL '1 minute' WHERE cron_configuration_id=$1").bind(bad).execute(&fixture.pool()).await.unwrap();
    sqlx::query("UPDATE cron_configurations SET parameter='invalid' WHERE id=$1")
        .bind(bad)
        .execute(&fixture.pool())
        .await
        .unwrap();
    let good = queue(&fixture, &client, false).await;
    assert_eq!(trigger(&client).await.status(), 400);
    let bad_state: (String, Option<chrono::NaiveDateTime>) =
        sqlx::query_as("SELECT status,lease_until FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(bad)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(bad_state.0, "FAILED");
    assert!(bad_state.1.is_none());
    let (id, status): (Uuid, Option<String>) = sqlx::query_as("SELECT job.id,audit.status FROM cron_jobs job LEFT JOIN report_results audit ON audit.id=job.id WHERE job.cron_configuration_id=$1").bind(good).fetch_one(&fixture.pool()).await.unwrap();
    fixture.own_report(id);
    assert_eq!(status.as_deref(), Some("SUCCESS"));
    assert_eq!(mail.requests.lock().unwrap().len(), 1);
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
    assert_eq!(failed["files"].as_array().unwrap().len(), 1);
    let response = reqwest::get(failed["files"][0]["download_url"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let id = Uuid::parse_str(failed["id"].as_str().unwrap()).unwrap();
    let patched = client
        .send(authed_json(
            "PATCH",
            &format!("/api/v1/admin/cron/configs/{config}"),
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
    assert_eq!(requests[0].1["from"], "reporting@enzumeii.com");
    let generated_at =
        chrono::DateTime::parse_from_rfc3339(failed["generated_at"].as_str().unwrap())
            .unwrap()
            .with_timezone(&chrono_tz::Asia::Bangkok)
            .format("%Y-%m-%d %H:%M:%S %:z")
            .to_string();
    assert_eq!(
        requests[0].1["subject"],
        format!("Inventory report - inventory ID 12345 - Generated: {generated_at} (Asia/Bangkok)")
    );
    assert!(requests[0].1["html"].as_str().unwrap().contains(&format!(
        "Report generation date: {generated_at} (Asia/Bangkok)"
    )));
    assert!(
        requests[0].1["html"]
            .as_str()
            .unwrap()
            .contains("Inventory ID: 12345")
    );
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
            &format!("/api/v1/admin/cron/configs/{config}"),
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
            &format!("/api/v1/admin/cron/reports/{id}"),
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
            &format!("/api/v1/admin/cron/reports/{id}"),
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
    let listed=body_json(get(&client,"/api/v1/admin/cron/reports?report_type=TRANSACTION&generated_from=2026-10-01T00%3A00%3A00%2B07%3A00&generated_to=2026-11-02T00%3A00%3A00%2B07%3A00&per_page=1",fixture.root_id()).await).await;
    assert_eq!(listed["items"][0]["id"], id.to_string());
    assert_eq!(listed["has_next"], false);
    assert_eq!(
        get(
            &client,
            &format!("/api/v1/admin/cron/reports/{id}/download"),
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
                &format!("/api/v1/admin/cron/reports?{query}"),
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
            "/api/v1/admin/cron/reports/not-a-uuid",
            fixture.root_id()
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        get(
            &client,
            &format!("/api/v1/admin/cron/reports/{}", Uuid::new_v4()),
            fixture.root_id()
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        get(&client, "/api/v1/admin/report-results", fixture.root_id())
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn cron_provider_failures_return_errors_and_persist_retryable_audits() {
    let fixture = fixture().await;
    for (failure, expected) in [
        ("http", 502),
        ("graphql", 502),
        ("malformed", 502),
        ("missing", 502),
        ("shape", 502),
        ("timeout", 504),
    ] {
        let (url, upstream) = crate::common::scripted_upstream(vec![], failure).await;
        let client = crate::common::client_with_scheduler(
            crate::common::ProviderConfig {
                shopify_url: Some(&url),
                ..Default::default()
            },
            60,
        )
        .await;
        let config = queue(&fixture, &client, false).await;
        let response = client
            .send(
                axum::http::Request::builder()
                    .uri("/_cron")
                    .header("authorization", "Bearer e2e-cron-secret")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected, "{failure}");
        let body = body_json(response).await;
        assert!(!body.to_string().contains("private"));
        let (id, status, attempts): (Uuid, String, i32) = sqlx::query_as("SELECT job.id,job.status,job.attempts FROM cron_jobs job WHERE cron_configuration_id=$1")
            .bind(config).fetch_one(&fixture.pool()).await.unwrap();
        fixture.own_report(id);
        assert_eq!(status, "FAILED", "{failure}");
        assert_eq!(attempts, 1, "{failure}");
        assert_ne!(upstream.requests().len(), 0);
    }
}

#[tokio::test]
async fn overlapping_cron_requests_share_claims_and_preserve_lease_renewal() {
    let fixture = fixture().await;
    let (url, _upstream) = inventory_upstream().await;
    let mail = ReportMail::start("slow").await;
    let client = worker(&url, &mail.url, 10).await;
    let config = queue(&fixture, &client, false).await;
    let (one, two) = tokio::join!(trigger(&client), trigger(&client));
    assert_eq!(one.status(), 204);
    assert_eq!(two.status(), 204);
    let (id, count): (Uuid, i32) =
        sqlx::query_as("SELECT id,attempts FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(config)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    fixture.own_report(id);
    assert_eq!(count, 1);
    assert_eq!(mail.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invocation_deadline_cancels_work_and_later_http_trigger_recovers_checkpoint() {
    let fixture = fixture().await;
    let (url, _upstream) = inventory_upstream().await;
    let mail = ReportMail::start("slow").await;
    let limited = crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(&url),
            resend_url: Some(&mail.url),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(10),
            invocation_budget_secs: Some(6),
            ..Default::default()
        },
        60,
    )
    .await;
    let config = queue(&fixture, &limited, false).await;
    let response = trigger(&limited).await;
    assert_eq!(response.status(), 503);
    assert!(
        body_json(response).await["error"]
            .as_str()
            .unwrap()
            .contains("invocation capacity")
    );
    let (id, initial_attempts): (Uuid, i32) =
        sqlx::query_as("SELECT id,attempts FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(config)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    fixture.own_report(id);
    assert_eq!(initial_attempts, 1);
    // No worker may mark success after the invocation has returned.
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM cron_jobs WHERE id=$1")
        .bind(id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
    assert_eq!(status, "PENDING");
    let client = worker(&url, &mail.url, 10).await;
    assert_eq!(trigger(&client).await.status(), 204);
    assert_eq!(attempts(&fixture, id).await, 2);
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].0["idempotency-key"],
        requests[1].0["idempotency-key"]
    );
    assert_eq!(requests[0].1, requests[1].1);
}

#[tokio::test]
async fn cron_blob_operation_failures_preserve_checkpoints_and_never_send_email() {
    let fixture = fixture().await;
    for (operation, failure, expected) in [
        ("metadata", "http", 502),
        ("metadata", "malformed", 502),
        ("metadata", "timeout", 504),
        ("upload", "http", 502),
        ("upload", "malformed", 502),
        ("upload", "timeout", 504),
        ("recovery", "http", 502),
        ("recovery", "malformed", 502),
        ("recovery", "timeout", 504),
        ("download", "http", 502),
        ("download", "timeout", 504),
        ("download", "checksum", 409),
    ] {
        let (url, upstream) = inventory_upstream().await;
        let mail = ReportMail::start("ok").await;
        let blob = crate::blob_fixture::Fixture::new().await;
        blob.data.lock().unwrap().failure = Some((operation, failure));
        let client = crate::common::client_with_scheduler(
            crate::common::ProviderConfig {
                shopify_url: Some(&url),
                resend_url: Some(&mail.url),
                resend_key: Some("re_e2e-fixture"),
                email_timeout_secs: Some(10),
                blob_url: Some(&blob.base),
                ..Default::default()
            },
            86400,
        )
        .await;
        let config = queue(&fixture, &client, false).await;
        let response = trigger(&client).await;
        assert_eq!(
            response.status().as_u16(),
            expected,
            "{operation}/{failure}"
        );
        assert!(!body_json(response).await.to_string().contains("private"));
        let (id, status, checkpoint): (Uuid, String, serde_json::Value) = sqlx::query_as(
            "SELECT id,status,execution_checkpoint FROM cron_jobs WHERE cron_configuration_id=$1",
        )
        .bind(config)
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
        fixture.own_report(id);
        assert_eq!(status, "FAILED");
        assert!(checkpoint.is_object());
        let audit: (Option<chrono::NaiveDateTime>, serde_json::Value, String) =
            sqlx::query_as("SELECT generated_at,files,error FROM report_results WHERE id=$1")
                .bind(id)
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert!(audit.0.is_some());
        assert_eq!(audit.1, json!([]));
        assert_eq!(audit.2, "Stored report publication failed");
        assert_eq!(mail.requests.lock().unwrap().len(), 0);
        assert_eq!(upstream.requests().len(), 4);
        let calls = blob.data.lock().unwrap().calls.clone();
        let observed = if operation == "recovery" {
            "metadata"
        } else {
            operation
        };
        assert!(calls.iter().any(|call| call == observed));
        if operation == "upload" {
            assert_eq!(calls, ["metadata", "upload", "metadata"]);
        }
    }
}

#[tokio::test]
async fn cron_transaction_success_publishes_both_csvs_and_commits_mail_checkpoint() {
    let fixture = fixture().await;
    let (upstream, calls) = crate::v1::admin::transactions::Server::accounting(None).await;
    let mail = ReportMail::start("ok").await;
    let blob = crate::blob_fixture::Fixture::new().await;
    let client = transaction_worker(&upstream.url, &mail.url, &blob.base).await;
    let config = queue_transaction(&fixture, &client).await;
    let response = trigger(&client).await;
    let status = response.status();
    let body = if status.is_success() {
        Value::Null
    } else {
        body_json(response).await
    };
    assert_eq!(status, 204, "{body}; {}", client.logs());
    let (id, status, checkpoint): (Uuid, String, Value) = sqlx::query_as(
        "SELECT id,status,execution_checkpoint FROM cron_jobs WHERE cron_configuration_id=$1",
    )
    .bind(config)
    .fetch_one(&fixture.pool())
    .await
    .unwrap();
    fixture.own_report(id);
    assert_eq!(status, "SUCCESS");
    assert_eq!(checkpoint["mail"]["state"], "accepted", "{checkpoint}");
    let audit: (String, Option<chrono::NaiveDateTime>, Value, Option<String>) =
        sqlx::query_as("SELECT status,generated_at,files,error FROM report_results WHERE id=$1")
            .bind(id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(audit.0, "SUCCESS");
    assert!(audit.1.is_some());
    assert!(audit.3.is_none());
    assert_eq!(audit.2.as_array().unwrap().len(), 2);
    let data = blob.data.lock().unwrap();
    assert_eq!(data.puts.len(), 2);
    let transaction_csv = data
        .objects
        .iter()
        .find(|(key, _)| key.ends_with("/transactions.csv"))
        .unwrap()
        .1;
    let item_csv = data
        .objects
        .iter()
        .find(|(key, _)| key.ends_with("/items.csv"))
        .unwrap()
        .1;
    let transaction_csv = String::from_utf8(transaction_csv.clone()).unwrap();
    let item_csv = String::from_utf8(item_csv.clone()).unwrap();
    assert!(transaction_csv.starts_with("gateway,type,po_number,"));
    assert!(transaction_csv.contains("STRIPE,SALE,") && transaction_csv.contains("PAYPAL,SALE,"));
    assert!(transaction_csv.contains("ch_123") && transaction_csv.contains("pp_sale"));
    assert!(item_csv.starts_with("quantity,price,currency,") && item_csv.contains("Keychain"));
    drop(data);
    let requests = mail.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let attachments = requests[0].1["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0]["filename"], "transactions.csv");
    assert_eq!(attachments[1]["filename"], "items.csv");
    drop(requests);
    assert_eq!(upstream.finish().await.len(), calls);
}

#[tokio::test]
async fn cron_transaction_payment_failures_stop_before_checkpoint_publication_and_mail() {
    let fixture = fixture().await;
    for operation in [
        "/payouts",
        "/balance_transactions",
        "/charges/ch_123",
        "/refunds/re_123",
        "/v1/oauth2/token",
        "/v1/reporting/transactions",
    ] {
        for fault in [
            "http",
            "timeout",
            "malformed",
            "missing",
            "shape",
            "disconnect",
            "refused",
        ] {
            let (upstream, calls) =
                crate::v1::admin::transactions::Server::accounting(Some((operation, fault))).await;
            let mail = ReportMail::start("ok").await;
            let blob = crate::blob_fixture::Fixture::new().await;
            let client = transaction_worker(&upstream.url, &mail.url, &blob.base).await;
            let config = queue_transaction(&fixture, &client).await;
            let response = trigger(&client).await;
            assert_eq!(
                response.status().as_u16(),
                if fault == "timeout" { 504 } else { 502 },
                "{operation}/{fault}"
            );
            assert_eq!(
                body_json(response).await["error"],
                "Upstream request failed"
            );
            let (id, status, checkpoint, attempts): (Uuid, String, Option<Value>, i32) = sqlx::query_as("SELECT id,status,execution_checkpoint,attempts FROM cron_jobs WHERE cron_configuration_id=$1").bind(config).fetch_one(&fixture.pool()).await.unwrap();
            fixture.own_report(id);
            assert_eq!(status, "FAILED", "{operation}/{fault}");
            assert_eq!(attempts, 1);
            assert!(checkpoint.is_none(), "{operation}/{fault}");
            let audit: (String, Option<chrono::NaiveDateTime>, Value, String) = sqlx::query_as(
                "SELECT status,generated_at,files,error FROM report_results WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(audit.0, "FAILED");
            assert!(audit.1.is_none());
            assert_eq!(audit.2, json!([]));
            assert_eq!(audit.3, "Report generation failed");
            assert!(blob.data.lock().unwrap().calls.is_empty());
            assert!(mail.requests.lock().unwrap().is_empty());
            assert_eq!(upstream.finish().await.len(), calls, "{operation}/{fault}");
        }
    }
}

async fn transaction_worker(upstream: &str, mail: &str, blob: &str) -> TestClient {
    crate::common::client_with_scheduler(
        crate::common::ProviderConfig {
            shopify_url: Some(&format!("{upstream}/graphql.json")),
            stripe_url: Some(upstream),
            paypal_url: Some(upstream),
            resend_url: Some(mail),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(10),
            blob_url: Some(blob),
            ..Default::default()
        },
        86400,
    )
    .await
}

async fn queue_transaction(fixture: &TestFixture, client: &TestClient) -> Uuid {
    let response = client.send(authed_json("POST", "/api/v1/admin/cron/configs", fixture.root_id(), AdminRole::SuperAdmin,
        &json!({"name":format!("{}-transaction",fixture.prefix()),"report_type":"TRANSACTION","cron_expression":"0 0 1 * *","receiver_email":"reports@example.test","parameter":"PREVIOUS_MONTH","enabled":false}))).await.unwrap();
    assert_eq!(response.status(), 201);
    let config = Uuid::parse_str(body_json(response).await["id"].as_str().unwrap()).unwrap();
    sqlx::query("INSERT INTO cron_jobs (cron_configuration_id,scheduled_for) VALUES ($1,'2026-10-31 17:00:00')").bind(config).execute(&fixture.pool()).await.unwrap();
    config
}

async fn trigger(client: &TestClient) -> axum::response::Response {
    client
        .send(
            axum::http::Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
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
    let response=client.send(authed_json("POST","/api/v1/admin/cron/configs",fixture.root_id(),AdminRole::SuperAdmin,&json!({
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
    let response = client
        .send(
            axum::http::Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.status().is_success() || response.status().is_server_error(),
        "cron status: {}",
        response.status()
    );
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
                        &format!("/api/v1/admin/cron/reports/{id}"),
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
    let mut files = Vec::new();
    for name in ["transactions.csv", "items.csv"] {
        let response = reqwest::Client::new()
            .put(client.storage_base())
            .query(&[("pathname", format!("{id}/{name}"))])
            .bearer_auth("vercel_blob_rw_local_synthetic")
            .header("x-api-version", "13")
            .header("x-vercel-blob-access", "public")
            .header("x-add-random-suffix", "0")
            .header("x-allow-overwrite", "0")
            .header("x-content-type", "text/csv; charset=utf-8")
            .body("header\r\noriginal\r\n")
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let blob: Value = response.json().await.unwrap();
        files.push(json!({"filename":name,"content_type":blob["contentType"],"storage_key":blob["pathname"],"download_url":blob["downloadUrl"]}));
    }
    sqlx::query("INSERT INTO report_results (id,report_type,attempted_at,generated_at,status,parameters,files) VALUES ($1,'TRANSACTION','2026-11-01 00:00:00','2026-11-01 00:00:00','SUCCESS',$2,$3)")
        .bind(id).bind("2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00").bind(json!(files)).execute(&fixture.pool()).await.unwrap();
    id
}

struct ReportMail {
    url: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<(axum::http::HeaderMap, serde_json::Value)>>>,
    task: tokio::task::JoinHandle<()>,
}
impl ReportMail {
    async fn start(mode: &'static str) -> Self {
        use axum::{Json, Router, response::IntoResponse, routing::post};
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let router = Router::new().route("/emails",post(move |headers:axum::http::HeaderMap,Json(body):Json<serde_json::Value>| {
            let recorded = recorded.clone();
            async move {
                assert_eq!(headers["authorization"],"Bearer re_e2e-fixture");
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
            task,
        }
    }
}
impl Drop for ReportMail {
    fn drop(&mut self) {
        self.task.abort();
    }
}
