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
    let signed = download_response(
        &client,
        Uuid::parse_str(failed["id"].as_str().unwrap()).unwrap(),
        failed["files"][0]["filename"].as_str().unwrap(),
        fixture.root_id(),
    )
    .await;
    let response = reqwest::get(signed["url"].as_str().unwrap()).await.unwrap();
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
async fn history_has_metadata_and_request_time_urls_for_multiple_saved_csv_files() {
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
        assert!(file.get("download_url").is_none());
        let signed = download_response(
            &client,
            id,
            file["filename"].as_str().unwrap(),
            fixture.root_id(),
        )
        .await;
        let response = reqwest::get(signed["url"].as_str().unwrap()).await.unwrap();
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
async fn cron_s3_operation_failures_preserve_checkpoints_and_never_send_email() {
    let fixture = fixture().await;
    for (operation, failure, expected) in [
        ("metadata", "http", 502),
        ("metadata", "malformed", 502),
        ("metadata", "timeout", 504),
        ("upload", "http", 502),
        ("upload", "malformed", 409),
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
        let blob = crate::s3_fixture::Fixture::new().await;
        blob.data.lock().unwrap().failure = Some((operation, failure));
        let client = crate::common::client_with_scheduler(
            crate::common::ProviderConfig {
                shopify_url: Some(&url),
                resend_url: Some(&mail.url),
                resend_key: Some("re_e2e-fixture"),
                email_timeout_secs: Some(10),
                s3_url: Some(&blob.base),
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
            sqlx::query_as("SELECT generated_at,(SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY position),'[]'::jsonb) FROM report_result_files f WHERE f.report_result_id=r.id),error FROM report_results r WHERE id=$1")
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
    let blob = crate::s3_fixture::Fixture::new().await;
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
        sqlx::query_as("SELECT status,generated_at,(SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY position),'[]'::jsonb) FROM report_result_files f WHERE f.report_result_id=r.id),error FROM report_results r WHERE id=$1")
            .bind(id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(audit.0, "SUCCESS");
    assert!(audit.1.is_some());
    assert!(audit.3.is_none());
    assert_eq!(audit.2.as_array().unwrap().len(), 2);
    {
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
        assert!(
            transaction_csv.contains("STRIPE,SALE,") && transaction_csv.contains("PAYPAL,SALE,")
        );
        assert!(transaction_csv.contains("ch_123") && transaction_csv.contains("pp_sale"));
        assert!(item_csv.starts_with("quantity,price,currency,") && item_csv.contains("Keychain"));

        let requests = mail.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let attachments = requests[0].1["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0]["filename"], "transactions.csv");
        assert_eq!(attachments[1]["filename"], "items.csv");
    }
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
            let blob = crate::s3_fixture::Fixture::new().await;
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
                "SELECT status,generated_at,(SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY position),'[]'::jsonb) FROM report_result_files f WHERE f.report_result_id=r.id),error FROM report_results r WHERE id=$1",
            )
            .bind(id)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
            assert_eq!(audit.0, "FAILED");
            assert!(audit.1.is_none());
            assert_eq!(audit.2, json!([]));
            assert_eq!(audit.3, "Report generation failed");
            assert_eq!(
                blob.data.lock().unwrap().calls,
                [] as [std::string::String; 0]
            );
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
            s3_url: Some(blob),
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
    let client = crate::common::s3_client(client.storage_base());
    let mut files = Vec::new();
    for name in ["transactions.csv", "items.csv"] {
        let bytes = b"header\r\noriginal\r\n";
        let file = download_receipt(id, name, bytes);
        client
            .put(
                file["storage_key"].as_str().unwrap(),
                "text/csv; charset=utf-8",
                bytes.to_vec(),
            )
            .await
            .unwrap();
        files.push(file);
    }
    sqlx::query("INSERT INTO report_results (id,report_type,attempted_at,generated_at,status,parameters) VALUES ($1,'TRANSACTION','2026-11-01 00:00:00','2026-11-01 00:00:00','SUCCESS',$2)")
        .bind(id).bind("2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00").execute(&fixture.pool()).await.unwrap();
    insert_download_receipts(fixture, id, json!(files)).await;
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

fn download_path(id: impl std::fmt::Display, filename: &str) -> String {
    format!("/api/v1/admin/cron/reports/{id}/files/{filename}/download-url")
}
fn download_receipt(id: Uuid, filename: &str, bytes: &[u8]) -> Value {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(bytes));
    json!({"filename":filename,"content_type":"text/csv; charset=utf-8","bucket":"reports",
        "storage_key":format!("transaction/20261101/{id}/{digest}/{filename}"),"size_bytes":bytes.len(),"sha256":digest})
}
fn assert_download_logs_redacted(client: &TestClient) {
    for secret in [
        "X-Amz-Signature",
        "synthetic-task3-access",
        "synthetic-task3-secret",
        "synthetic-access-id",
        "synthetic-secret-key",
        "private S3 failure",
    ] {
        assert!(!client.logs().contains(secret));
    }
}
async fn insert_download_receipts(fixture: &TestFixture, id: Uuid, files: Value) {
    for file in files.as_array().unwrap() {
        let position = i32::from(file["filename"] == "items.csv");
        sqlx::query("INSERT INTO report_result_files (report_result_id,filename,position,content_type,bucket,storage_key,size_bytes,sha256) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(id).bind(file["filename"].as_str().unwrap()).bind(position)
            .bind(file["content_type"].as_str().unwrap()).bind(file["bucket"].as_str().unwrap())
            .bind(file["storage_key"].as_str().unwrap()).bind(file["size_bytes"].as_i64().unwrap())
            .bind(file["sha256"].as_str().unwrap()).execute(&fixture.pool()).await.unwrap();
    }
}
async fn replace_download_receipt(fixture: &TestFixture, id: Uuid, file: Value) {
    sqlx::query("DELETE FROM report_result_files WHERE report_result_id=$1")
        .bind(id)
        .execute(&fixture.pool())
        .await
        .unwrap();
    insert_download_receipts(fixture, id, json!([file])).await;
}
async fn download_audit(fixture: &TestFixture, id: Uuid, files: Value) {
    fixture.own_report(id);
    sqlx::query("INSERT INTO report_results (id,report_type,attempted_at,generated_at,status,parameters) VALUES ($1,'TRANSACTION','2026-11-01','2026-11-01','SUCCESS',$2)")
        .bind(id).bind("2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00").execute(&fixture.pool()).await.unwrap();
    insert_download_receipts(fixture, id, files).await;
}
async fn download_response(client: &TestClient, id: Uuid, filename: &str, admin: i64) -> Value {
    let response = get(client, &download_path(id, filename), admin).await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let result = body_json(response).await;
    assert_eq!(result.as_object().unwrap().len(), 2);
    result
}
async fn download_cookie(client: &TestClient, fixture: &TestFixture) -> String {
    let login = client
        .send(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("x-session-mode", "cookie")
                .header("origin", "http://127.0.0.1:8084")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    json!({"username":fixture.username("root"),"password":"fixture-root-password"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn download_url_invalid_session_mode_is_no_store_and_rejects_before_s3() {
    let fixture = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let client = crate::common::client_with_providers(crate::common::ProviderConfig {
        s3_url: Some(&storage.base),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let mut request = crate::common::authed(
        "GET",
        &download_path(Uuid::new_v4(), "transactions.csv"),
        fixture.root_id(),
        AdminRole::SuperAdmin,
    );
    request
        .headers_mut()
        .insert("x-session-mode", "invalid".parse().unwrap());
    let response = client.send(request).await.unwrap();
    let headers = response.headers().clone();
    crate::common::assert_report_error(response, 400, "Invalid session mode").await;
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}

#[tokio::test]
async fn download_url_authentication_and_shared_browser_guard_reject_before_s3() {
    let fixture = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let client = crate::common::client_with_providers(crate::common::ProviderConfig {
        s3_url: Some(&storage.base),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let path = download_path(Uuid::new_v4(), "transactions.csv");
    let admin = fixture.root_id();
    let cookie = download_cookie(&client, &fixture).await;
    // Safe GET requests preserve the existing browser middleware policy.
    let guard_response = client
        .send(
            axum::http::Request::builder()
                .method("POST")
                .uri(&path)
                .header("x-session-mode", "cookie")
                .header("origin", "http://untrusted.example.test")
                .header("cookie", &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(guard_response.status(), 403);
    crate::common::assert_report_auth(&client, &path).await;
    let invalid_cookie = client
        .send(
            axum::http::Request::builder()
                .uri(&path)
                .header("x-session-mode", "cookie")
                .header("cookie", "backoffice_auth=invalid")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    crate::common::assert_report_error(invalid_cookie, 401, "Invalid or expired token").await;
    for claims in [
        json!({"sub":admin,"username":"fixture","role":"USER","exp":4_102_444_800_u64}),
        json!({"sub":admin,"username":"fixture","role":"ADMIN","exp":1}),
    ] {
        let jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(
                &hex::decode(crate::common::JWT_SECRET).unwrap(),
            ),
        )
        .unwrap();
        let response = client
            .send(
                axum::http::Request::builder()
                    .uri(&path)
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        crate::common::assert_report_error(response, 401, "Invalid or expired token").await;
    }
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}

#[tokio::test]
async fn download_url_private_s3_bearer_cookie_and_fresh_expiration() {
    use crate::common::{authed, seed_admin};
    let fixture = fixture().await;
    let client = client().await;
    let id = seed(&fixture, &client).await;
    let path = download_path(id, "transactions.csv");
    let admin = seed_admin(
        &fixture,
        fixture.username("download-admin"),
        "synthetic-password",
        AdminRole::Admin,
    )
    .await;
    let response = client
        .send(authed("GET", &format!("{path}?bucket=other&key=untrusted.csv&ttl_secs=900&url=https%3A%2F%2Funtrusted.example.test"), admin, AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = body_json(response).await;
    let url = reqwest::Url::parse(body["url"].as_str().unwrap()).unwrap();
    assert_eq!(
        url.origin(),
        reqwest::Url::parse(client.storage_base()).unwrap().origin()
    );
    assert!(
        url.path()
            .starts_with(&format!("/reports/transaction/20261101/{id}/"))
    );
    let query = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(query["X-Amz-Expires"], "300");
    assert!(
        query["response-content-disposition"].contains("attachment; filename=\"transactions.csv\"")
    );
    let expires =
        chrono::DateTime::parse_from_rfc3339(body["expires_at"].as_str().unwrap()).unwrap();
    let signed_at = chrono::NaiveDateTime::parse_from_str(&query["X-Amz-Date"], "%Y%m%dT%H%M%SZ")
        .unwrap()
        .and_utc();
    assert_eq!(expires.timestamp(), signed_at.timestamp() + 300);
    let unsigned = reqwest::get(format!(
        "{}{}",
        client.storage_base().trim_end_matches('/'),
        url.path()
    ))
    .await
    .unwrap();
    assert!(matches!(unsigned.status().as_u16(), 400 | 403));
    let file = reqwest::get(url.clone()).await.unwrap();
    assert_eq!(file.status(), 200);
    assert!(
        file.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("attachment")
    );
    assert_eq!(file.bytes().await.unwrap(), "header\r\noriginal\r\n");
    let cookie = download_cookie(&client, &fixture).await;
    let cookie_response = client
        .send(
            axum::http::Request::builder()
                .uri(&path)
                .header("x-session-mode", "cookie")
                .header("origin", "http://127.0.0.1:8084")
                .header("cookie", &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cookie_response.status(), 200);
    assert_eq!(cookie_response.headers()["cache-control"], "no-store");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let renewed = download_response(&client, id, "transactions.csv", fixture.root_id()).await;
    assert!(
        chrono::DateTime::parse_from_rfc3339(renewed["expires_at"].as_str().unwrap()).unwrap()
            > expires
    );
    let detail = body_json(
        get(
            &client,
            &format!("/api/v1/admin/cron/reports/{id}"),
            fixture.root_id(),
        )
        .await,
    )
    .await;
    assert_eq!(detail["files"][0].as_object().unwrap().len(), 2);
    assert!(detail["files"][0].get("download_url").is_none());
    let persisted: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(f) ORDER BY position),'[]'::jsonb) FROM report_result_files f WHERE report_result_id=$1")
        .bind(id)
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
    assert!(!persisted.to_string().contains("X-Amz"));
    assert_download_logs_redacted(&client);
}
#[tokio::test]
async fn download_url_validation_receipt_identity_and_absence_fail_before_s3() {
    let fixture = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let client = crate::common::client_with_providers(crate::common::ProviderConfig {
        s3_url: Some(&storage.base),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let id = Uuid::new_v4();
    let other = Uuid::new_v4();
    download_audit(
        &fixture,
        other,
        json!([download_receipt(other, "transactions.csv", b"other")]),
    )
    .await;
    let original = download_receipt(id, "transactions.csv", b"x");
    download_audit(&fixture, id, json!([original.clone()])).await;
    for (report, filename) in [
        ("invalid".to_owned(), "transactions.csv".to_owned()),
        (id.to_string(), ".csv".into()),
        (id.to_string(), "report.txt".into()),
        (id.to_string(), "a%2Fb.csv".into()),
        (id.to_string(), format!("{}.csv", "a".repeat(201))),
        (id.to_string(), "report%20name.csv".into()),
    ] {
        let response = get(
            &client,
            &download_path(report, &filename),
            fixture.root_id(),
        )
        .await;
        assert_eq!(response.status(), 400, "{filename}");
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    for (report, filename, message) in [
        (
            Uuid::new_v4(),
            "transactions.csv",
            "Report result not found",
        ),
        (id, "inventory.csv", "Report file not found"),
    ] {
        crate::common::assert_report_error(
            get(&client, &download_path(report, filename), fixture.root_id()).await,
            404,
            message,
        )
        .await;
    }
    for (field, value) in [
        ("bucket", json!("other-reports")),
        ("sha256", json!("a".repeat(64))),
        (
            "storage_key",
            json!(
                original["storage_key"]
                    .as_str()
                    .unwrap()
                    .replace(&id.to_string(), &other.to_string())
            ),
        ),
        (
            "storage_key",
            json!(
                original["storage_key"]
                    .as_str()
                    .unwrap()
                    .replace("20261101", "20261102")
            ),
        ),
        ("content_type", json!("application/json")),
    ] {
        let mut file = original.clone();
        file[field] = value;
        replace_download_receipt(&fixture, id, file).await;
        let response = get(
            &client,
            &download_path(id, "transactions.csv"),
            fixture.root_id(),
        )
        .await;
        crate::common::assert_report_error(response, 500, "Saved report metadata is invalid").await;
    }
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
    replace_download_receipt(&fixture, id, original).await;
    crate::common::assert_report_error(
        get(
            &client,
            &download_path(id, "transactions.csv"),
            fixture.root_id(),
        )
        .await,
        404,
        "Report file not found",
    )
    .await;
    assert_eq!(storage.data.lock().unwrap().calls, ["metadata"]);
}
#[tokio::test]
async fn download_url_head_failures_metadata_and_local_configuration_are_distinct() {
    let fixture = fixture().await;
    let id = Uuid::new_v4();
    let original = download_receipt(id, "transactions.csv", b"x");
    download_audit(&fixture, id, json!([original.clone()])).await;
    for (fault, status) in [
        ("http", 502),
        ("malformed", 502),
        ("timeout", 504),
        ("size", 502),
        ("type", 502),
        ("ok", 200),
    ] {
        let storage = crate::s3_fixture::Fixture::new().await;
        storage
            .client
            .put(
                original["storage_key"].as_str().unwrap(),
                "text/csv; charset=utf-8",
                if fault == "size" {
                    b"xx".to_vec()
                } else {
                    b"x".to_vec()
                },
            )
            .await
            .unwrap();
        {
            let mut data = storage.data.lock().unwrap();
            data.calls.clear();
            if matches!(fault, "http" | "malformed" | "timeout") {
                data.failure = Some(("metadata", fault));
            }
            if fault == "type" {
                data.content_type = Some("application/json".into());
            }
        }
        let client = crate::common::client_with_providers(crate::common::ProviderConfig {
            s3_url: Some(&storage.base),
            scheduler_enabled: Some(false),
            s3_ttl_secs: Some(1),
            ..Default::default()
        })
        .await;
        let response = get(
            &client,
            &download_path(id, "transactions.csv"),
            fixture.root_id(),
        )
        .await;
        assert_eq!(response.status(), status, "{fault}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = body_json(response).await;
        if status == 200 {
            assert_eq!(
                reqwest::Url::parse(body["url"].as_str().unwrap())
                    .unwrap()
                    .query_pairs()
                    .find(|(key, _)| key == "X-Amz-Expires")
                    .unwrap()
                    .1,
                "1"
            );
        } else {
            assert_eq!(body, json!({"error":"Upstream request failed"}));
        }
        assert_eq!(
            storage.data.lock().unwrap().calls,
            ["metadata"],
            "local signing makes no remote call"
        );
        for secret in [
            "private S3 failure",
            "synthetic-access-id",
            "synthetic-secret-key",
            "X-Amz-Signature",
        ] {
            assert!(!client.logs().contains(secret));
            if status != 200 {
                assert!(!body.to_string().contains(secret));
            }
        }
    }
}

#[tokio::test]
async fn download_url_local_configuration_and_connection_failures_are_safe() {
    let fixture = fixture().await;
    let id = Uuid::new_v4();
    download_audit(
        &fixture,
        id,
        json!([download_receipt(id, "transactions.csv", b"x")]),
    )
    .await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = crate::common::client_with_providers(crate::common::ProviderConfig {
        s3_url: Some(&endpoint),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    crate::common::assert_report_error(
        get(
            &client,
            &download_path(id, "transactions.csv"),
            fixture.root_id(),
        )
        .await,
        502,
        "Upstream request failed",
    )
    .await;
    for (disabled, invalid, ttl) in [
        (true, false, 300),
        (false, true, 300),
        (false, false, 0),
        (false, false, 901),
    ] {
        let storage = crate::s3_fixture::Fixture::new().await;
        let client = crate::common::client_with_providers(crate::common::ProviderConfig {
            s3_url: Some(&storage.base),
            s3_disabled: disabled,
            s3_invalid_credentials: invalid,
            s3_ttl_secs: Some(ttl),
            scheduler_enabled: Some(false),
            ..Default::default()
        })
        .await;
        crate::common::assert_report_error(
            get(
                &client,
                &download_path(id, "transactions.csv"),
                fixture.root_id(),
            )
            .await,
            503,
            "S3 client unavailable",
        )
        .await;
        assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
    }
}

#[tokio::test]
async fn download_url_database_read_failure_is_safe_and_does_not_call_s3() {
    let fixture = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let role = format!("{}_download", fixture.prefix());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD 'synthetic-download-password'; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT ON _sqlx_migrations TO {role};")))
        .execute(&fixture.pool()).await.unwrap();
    let mut url = reqwest::Url::parse(&crate::common::database_url()).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("synthetic-download-password"))
        .unwrap();
    let client = crate::common::client_with_providers(crate::common::ProviderConfig {
        database_url: Some(url.as_str()),
        s3_url: Some(&storage.base),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let response = get(
        &client,
        &download_path(Uuid::new_v4(), "transactions.csv"),
        fixture.root_id(),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    let logs = client.logs();
    drop(client);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE SELECT ON _sqlx_migrations FROM {role}; REVOKE USAGE ON SCHEMA public FROM {role}; DROP ROLE {role};")))
        .execute(&fixture.pool()).await.unwrap();
    assert_eq!(status, 500);
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(body, json!({"error":"Internal server error"}));
    assert!(!logs.contains("permission denied"));
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}

#[tokio::test]
async fn rustfs_private_network_acceptance() {
    let api = crate::common::client().await;
    let client = crate::common::s3_client(api.storage_base());
    let key = format!("acceptance/{}/with spaces+%/ไทย.csv", uuid::Uuid::new_v4());
    assert!(client.metadata(&key).await.unwrap().is_none());
    client
        .put(&key, "text/csv; charset=utf-8", b"original".to_vec())
        .await
        .unwrap();
    let metadata = client.metadata(&key).await.unwrap().unwrap();
    assert_eq!(metadata.size, 8);
    assert_eq!(metadata.content_type, "text/csv; charset=utf-8");
    assert_eq!(client.download(&key, 8).await.unwrap(), b"original");
    let signed = client
        .presign_download(&key, "synthetic_report.csv", 1)
        .await
        .unwrap();
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = http.get(&signed.url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_DISPOSITION],
        "attachment; filename=\"synthetic_report.csv\""
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"original");
    let mut unsigned = reqwest::Url::parse(&signed.url).unwrap();
    unsigned.set_query(None);
    assert_eq!(
        http.get(unsigned).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        http.get(&signed.url).send().await.unwrap().status(),
        reqwest::StatusCode::FORBIDDEN
    );
    client
        .put(&key, "text/csv; charset=utf-8", b"replaced".to_vec())
        .await
        .unwrap();
    assert_eq!(client.download(&key, 8).await.unwrap(), b"replaced");
    let (a, b) = tokio::join!(
        client.put(&key, "text/csv; charset=utf-8", b"frozen".to_vec()),
        client.put(&key, "text/csv; charset=utf-8", b"frozen".to_vec())
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(client.download(&key, 6).await.unwrap(), b"frozen");
}

#[tokio::test]
async fn history_pages_parents_with_ordered_receipts_and_empty_reports() {
    let fixture = fixture().await;
    let client = client().await;
    let mut ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    ids.sort_unstable();
    download_audit(&fixture, ids[0], json!([])).await;
    download_audit(&fixture, ids[1], json!([])).await;
    download_audit(
        &fixture,
        ids[2],
        json!([
            download_receipt(ids[2], "items.csv", b"items"),
            download_receipt(ids[2], "transactions.csv", b"transactions")
        ]),
    )
    .await;
    for (page, id, has_next) in [(1, ids[2], true), (2, ids[1], true), (3, ids[0], false)] {
        let path = format!(
            "/api/v1/admin/cron/reports?generated_from=2026-11-01T00%3A00%3A00Z&generated_to=2026-11-01T00%3A00%3A01Z&per_page=1&page={page}"
        );
        let response = get(&client, &path, fixture.root_id()).await;
        assert_eq!(response.status(), 200);
        let body = body_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["id"], id.to_string());
        assert_eq!(body["has_next"], has_next);
        let detail = get(
            &client,
            &format!("/api/v1/admin/cron/reports/{id}"),
            fixture.root_id(),
        )
        .await;
        assert_eq!(detail.status(), 200);
        let detail = body_json(detail).await;
        assert_eq!(body["items"][0], detail);
        if page == 1 {
            assert_eq!(
                detail["files"],
                json!([
                    {"filename":"transactions.csv", "content_type":"text/csv; charset=utf-8"},
                    {"filename":"items.csv", "content_type":"text/csv; charset=utf-8"}
                ])
            );
        } else {
            assert_eq!(detail["files"], json!([]));
            crate::common::assert_report_error(
                get(&client, &download_path(id, "items.csv"), fixture.root_id()).await,
                404,
                "Report file not found",
            )
            .await;
        }
    }
}
