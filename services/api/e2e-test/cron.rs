//! GET /_cron through the owned API process and real Compose PostgreSQL.
mod checkout;

use crate::common::{self, ProviderConfig};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};

#[tokio::test]
async fn validates_cron_authentication_and_method_before_work() {
    let _fixture = common::fixture().await;
    let client = common::client().await;
    for authorization in [
        None,
        Some("Bearer wrong-secret"),
        Some("Basic e2e-cron-secret"),
        Some("Bearer"),
        Some("bearer e2e-cron-secret"),
    ] {
        let mut request = Request::builder().uri("/_cron");
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let response = client
            .send(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert_eq!(common::body_json(response).await["error"], "Not authorized");
    }
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .method("POST")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .method("HEAD")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn unconfigured_cron_secret_fails_closed() {
    let _fixture = common::fixture().await;
    let client = common::client_with_providers(ProviderConfig {
        cron_secret: Some(""),
        ..Default::default()
    })
    .await;
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        common::body_json(response).await["error"],
        "Cron authentication unavailable"
    );
}

#[tokio::test]
async fn retention_removes_expired_and_revoked_tokens_and_preserves_active_tokens() {
    let fixture = common::fixture().await;
    for (label, expired, revoked) in [
        ("expired", true, false),
        ("revoked", false, true),
        ("active", false, false),
    ] {
        sqlx::query("INSERT INTO refresh_tokens (admin_id, token_hash, expires_at, revoked_at) VALUES ($1,$2,NOW()+($3 * INTERVAL '1 day'),CASE WHEN $4 THEN NOW() ELSE NULL END)")
            .bind(fixture.root_id()).bind(fixture.username(label)).bind(if expired {-1_i32} else {1}).bind(revoked)
            .execute(&fixture.pool()).await.unwrap();
    }
    let client = common::client().await;
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let hashes: Vec<String> =
        sqlx::query_scalar("SELECT token_hash FROM refresh_tokens WHERE admin_id=$1")
            .bind(fixture.root_id())
            .fetch_all(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(hashes, vec![fixture.username("active")]);
}

#[tokio::test]
async fn server_skips_migrations_and_cron_returns_database_error_without_spinning() {
    let fixture = common::fixture().await;
    let configuration: uuid::Uuid = sqlx::query_scalar("INSERT INTO cron_configurations (name,report_type,cron_expression,receiver_email,parameter,enabled) VALUES ($1,'INVENTORY','* * * * *','synthetic@example.test','12345',false) RETURNING id")
        .bind(format!("{}-readonly", fixture.prefix())).fetch_one(&fixture.pool()).await.unwrap();
    sqlx::query("INSERT INTO cron_jobs (cron_configuration_id,scheduled_for) VALUES ($1,CURRENT_TIMESTAMP AT TIME ZONE 'UTC')")
        .bind(configuration).execute(&fixture.pool()).await.unwrap();
    let mut url = reqwest::Url::parse(&common::database_url()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", "-cdefault_transaction_read_only=on");
    let client = common::client_with_providers(ProviderConfig {
        database_url: Some(url.as_str()),
        ..Default::default()
    })
    .await;
    let started = std::time::Instant::now();
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(
        common::body_json(response).await["error"],
        "Internal server error"
    );
    let attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(attempts, 0);
}

#[tokio::test]
async fn daily_http_dispatch_preserves_bangkok_slots_inventory_coalescing_and_catchup_limit() {
    let fixture = common::fixture().await;
    let inventory: uuid::Uuid = sqlx::query_scalar("INSERT INTO cron_configurations (name,report_type,cron_expression,receiver_email,parameter,enabled,updated_at) VALUES ($1,'INVENTORY','* * * * *','synthetic@example.test','12345',true,(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1500 minutes') RETURNING id")
        .bind(format!("{}-inventory", fixture.prefix())).fetch_one(&fixture.pool()).await.unwrap();
    let transaction: uuid::Uuid = sqlx::query_scalar("INSERT INTO cron_configurations (name,report_type,cron_expression,receiver_email,parameter,enabled,updated_at) VALUES ($1,'TRANSACTION','0 0 1 * *','synthetic@example.test','PREVIOUS_MONTH',true,date_trunc('month',CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Bangkok')-INTERVAL '3 months'+INTERVAL '1 day'-INTERVAL '7 hours') RETURNING id")
        .bind(format!("{}-transaction", fixture.prefix())).fetch_one(&fixture.pool()).await.unwrap();
    let response = common::client()
        .await
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (count, minutes): (i64, f64) = sqlx::query_as("SELECT COUNT(*),EXTRACT(EPOCH FROM (MAX(job.scheduled_for)-MAX(config.updated_at)))::float8/60 FROM cron_jobs job JOIN cron_configurations config ON job.cron_configuration_id=config.id WHERE config.id=$1")
        .bind(inventory).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!(count, 1);
    assert!(minutes > 999.0 && minutes <= 1000.0);
    let rows: Vec<(uuid::Uuid, chrono::NaiveDateTime, String)> = sqlx::query_as("SELECT job.id,job.scheduled_for,audit.parameters FROM cron_jobs job JOIN report_results audit ON audit.id=job.id WHERE cron_configuration_id=$1 ORDER BY scheduled_for")
        .bind(transaction).fetch_all(&fixture.pool()).await.unwrap();
    assert_eq!(rows.len(), 3);
    for (id, scheduled, parameters) in rows {
        fixture.own_report(id);
        let due = scheduled.and_utc().with_timezone(&chrono_tz::Asia::Bangkok);
        let end = due.date_naive().pred_opt().unwrap();
        let start = due
            .date_naive()
            .checked_sub_months(chrono::Months::new(1))
            .unwrap();
        assert_eq!(
            parameters,
            format!("{start}T00:00:00+07:00 - {end}T23:59:59+07:00")
        );
    }
}

#[tokio::test]
async fn vercel_has_no_background_work_and_disabled_tick_retains_without_claiming() {
    let fixture = common::fixture().await;
    let configuration = seed_due(&fixture, "idle", true).await;
    let client = common::client_with_providers(ProviderConfig::default()).await;
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
    sqlx::query("INSERT INTO cron_jobs (cron_configuration_id,scheduled_for) VALUES ($1,CURRENT_TIMESTAMP AT TIME ZONE 'UTC')").bind(configuration).execute(&fixture.pool()).await.unwrap();
    expired_token(&fixture).await;
    // No startup retention under Vercel either.
    assert_eq!(token_count(&fixture).await, 1);
    let disabled = common::client_with_providers(ProviderConfig {
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let response = disabled
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(token_count(&fixture).await, 0);
    let (count, attempts): (i64, i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(attempts),0)::bigint FROM cron_jobs WHERE cron_configuration_id=$1").bind(configuration).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!((count, attempts), (1, 0));
    drop(client);
}

#[tokio::test]
async fn vps_periodic_dispatch_continues_after_a_failed_tick_and_stops_owned_tasks() {
    let fixture = common::fixture().await;
    let bad = seed_due(&fixture, "invalid", false).await;
    sqlx::query("UPDATE cron_configurations SET parameter='invalid' WHERE id=$1")
        .bind(bad)
        .execute(&fixture.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO cron_jobs (cron_configuration_id,scheduled_for) VALUES ($1,CURRENT_TIMESTAMP AT TIME ZONE 'UTC')").bind(bad).execute(&fixture.pool()).await.unwrap();
    let client = common::client_with_providers(ProviderConfig {
        vps_cron: true,
        ..Default::default()
    })
    .await;
    wait_job(&fixture, bad, "FAILED").await;
    assert!(client.logs().contains("cron invocation"));
    let good = seed_due(&fixture, "valid-after-failure", true).await;
    wait_job(&fixture, good, "FAILED").await;
    let audit: (String, Option<String>) = sqlx::query_as("SELECT audit.status,audit.error FROM report_results audit JOIN cron_jobs job ON audit.id=job.id WHERE job.cron_configuration_id=$1").bind(good).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!(
        audit,
        ("FAILED".into(), Some("Report generation failed".into()))
    );
    let response = client
        .send(
            Request::builder()
                .uri("/_cron")
                .header("authorization", "Bearer e2e-cron-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let (success, logs) = client.shutdown();
    assert!(success, "{logs}");
    assert!(logs.contains("Cron runtime tasks stopped"));
    let attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(good)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let after: i32 =
        sqlx::query_scalar("SELECT attempts FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(good)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(after, attempts);
}

#[tokio::test]
async fn vps_retention_continues_with_reports_disabled_and_does_not_claim() {
    let fixture = common::fixture().await;
    let configuration = seed_due(&fixture, "disabled-vps", true).await;
    let client = common::client_with_providers(ProviderConfig {
        vps_cron: true,
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    expired_token(&fixture).await;
    tokio::time::timeout(std::time::Duration::from_secs(4), async {
        while token_count(&fixture).await != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert!(client.shutdown().0);
}

#[tokio::test]
async fn vps_retention_runs_independently_of_a_slow_report_and_shutdown_cancels_work() {
    let fixture = common::fixture().await;
    let (url, _upstream) = common::scripted_upstream(vec![], "timeout").await;
    let configuration = seed_due(&fixture, "slow-vps", true).await;
    let client = common::client_with_providers(ProviderConfig {
        vps_cron: true,
        shopify_url: Some(&url),
        ..Default::default()
    })
    .await;
    wait_job(&fixture, configuration, "PENDING").await;
    expired_token(&fixture).await;
    tokio::time::timeout(std::time::Duration::from_millis(1800), async {
        while token_count(&fixture).await != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let status: String =
        sqlx::query_scalar("SELECT status FROM cron_jobs WHERE cron_configuration_id=$1")
            .bind(configuration)
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(status, "PENDING");
    let (success, logs) = client.shutdown();
    assert!(success, "{logs}");
    assert!(logs.contains("Cron runtime tasks stopped"));
    assert!(!logs.contains("cron job completed"));
}

async fn seed_due(fixture: &common::TestFixture, label: &str, enabled: bool) -> uuid::Uuid {
    sqlx::query_scalar("INSERT INTO cron_configurations (name,report_type,cron_expression,receiver_email,parameter,enabled,updated_at) VALUES ($1,'INVENTORY','* * * * *','synthetic@example.test','12345',$2,(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '61 seconds') RETURNING id")
        .bind(format!("{}-{label}", fixture.prefix())).bind(enabled).fetch_one(&fixture.pool()).await.unwrap()
}

async fn wait_job(fixture: &common::TestFixture, configuration: uuid::Uuid, status: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            let found: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cron_jobs WHERE cron_configuration_id=$1 AND status=$2 AND ($2 <> 'PENDING' OR lease_token IS NOT NULL))").bind(configuration).bind(status).fetch_one(&fixture.pool()).await.unwrap();
            if found { break; }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }).await.unwrap();
}

async fn expired_token(fixture: &common::TestFixture) {
    sqlx::query("INSERT INTO refresh_tokens (admin_id,token_hash,expires_at) VALUES ($1,$2,NOW()-INTERVAL '1 day')").bind(fixture.root_id()).bind(fixture.username("expired-lifecycle")).execute(&fixture.pool()).await.unwrap();
}

async fn token_count(fixture: &common::TestFixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE admin_id=$1")
        .bind(fixture.root_id())
        .fetch_one(&fixture.pool())
        .await
        .unwrap()
}
