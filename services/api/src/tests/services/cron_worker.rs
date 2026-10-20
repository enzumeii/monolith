#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn missing_or_malformed_email_acceptance_is_uncertain() {
    use crate::client::{ClientError, ClientErrorKind};
    assert!(email_outcome_is_uncertain(&ClientError::new(
        ClientErrorKind::Protocol("email result missing")
    )));
    assert!(email_outcome_is_uncertain(&ClientError::new(
        ClientErrorKind::Upstream {
            source: Box::new(resend_rs::Error::Parse {
                message: "synthetic response".into(),
                source: None
            }),
            timed_out: false,
        }
    )));
    assert!(!email_outcome_is_uncertain(&ClientError::new(
        ClientErrorKind::Upstream {
            source: Box::new(resend_rs::Error::RateLimit {
                ratelimit_limit: None,
                ratelimit_remaining: None,
                ratelimit_reset: None
            }),
            timed_out: false,
        }
    )));
}

#[tokio::test]
async fn cancelled_in_flight_email_remains_uncertain_in_the_audit() {
    use crate::repositories::cron::tests::{generated_job, isolated_state};
    use std::sync::Arc;
    let (mut state, directory) = isolated_state().await;
    let mut settings = (*state.settings).clone();
    settings.scheduler.lease_secs = 3;
    settings.scheduler.max_generation_attempts = 1;
    settings.shipping_sender = Some(crate::config::ShippingSender {
        name: "Synthetic".into(),
        phone: "0000000000".into(),
        email: "sender@example.test".into(),
        address: "Synthetic".into(),
        sub_district: "Synthetic".into(),
        district: "Synthetic".into(),
        province: "Synthetic".into(),
        postcode: "10100".into(),
        country_code: "TH".into(),
    });
    state.settings = Arc::new(settings);
    let id = generated_job(&state).await;
    super::report_storage::prepare(
        &directory,
        id,
        Utc::now().naive_utc(),
        domain::dto::ReportFileFormat::Csv,
        vec![super::report_storage::GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: super::report_storage::CSV_CONTENT_TYPE.into(),
            bytes: b"sku\r\n".to_vec(),
        }],
    )
    .await
    .unwrap();
    let (client, server) = email_server_that_rejects_renewal(state.pool.clone()).await;
    state.resend = Ok(Some(Arc::new(client)));
    tokio::time::timeout(std::time::Duration::from_secs(4), send_one(&state))
        .await
        .unwrap()
        .unwrap();
    let audit: (String, Option<String>) =
        sqlx::query_as("SELECT email_status,email_error FROM report_results WHERE id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(
        audit.0, "SENDING",
        "cancelled transport cannot prove email rejection"
    );
    assert_eq!(audit.1.as_deref(), Some("Email delivery attempt failed"));
    let expired_at = sqlx::query_scalar::<_, chrono::NaiveDateTime>(
        "SELECT lease_until FROM cron_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    repositories::cron::recover_expired(&state.pool, expired_at, 1)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT email_status FROM report_results WHERE id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        "UNKNOWN"
    );
    server.abort();
    state.pool.close().await;
    tokio::fs::remove_dir_all(directory).await.unwrap();
}

async fn email_server_that_rejects_renewal(
    pool: PgPool,
) -> (
    crate::client::resend::ResendClient,
    tokio::task::JoinHandle<()>,
) {
    use axum::{Json, Router, routing::post};
    let router = Router::new().route("/emails",post(move || {
        let pool = pool.clone();
        async move {
            sqlx::raw_sql("CREATE FUNCTION pg_temp.reject_email_renewal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.email_status='SENDING' AND NEW.email_status='SENDING' AND NEW.lease_token=OLD.lease_token AND NEW.lease_until IS DISTINCT FROM OLD.lease_until THEN RAISE EXCEPTION 'owned renewal fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_email_renewal BEFORE UPDATE ON cron_jobs FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_email_renewal();")
                .execute(&pool).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            Json(serde_json::json!({"id":"accepted-but-response-interrupted"}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let client = crate::client::resend::tests::client_with_url(url);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (client, server)
}

#[tokio::test]
async fn heartbeat_keeps_polling_work_while_waiting_for_its_database_connection() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(
            &std::env::var("E2E_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into()),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE cron_jobs (id UUID PRIMARY KEY, lease_token UUID NOT NULL, lease_until TIMESTAMP NOT NULL)")
        .execute(&pool).await.unwrap();
    let id = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query("INSERT INTO cron_jobs VALUES ($1,$2,(CURRENT_TIMESTAMP AT TIME ZONE 'UTC') + INTERVAL '30 seconds')")
        .bind(id).bind(token).execute(&pool).await.unwrap();
    let settings = serde_json::from_value(serde_json::json!({
        "server":{"host":"127.0.0.1","port":0},
        "database":{"url":"postgres://unused","max_connections":1,"min_connections":1},
        "browser":{"public_origin":"http://127.0.0.1:8084","insecure_local":true},
        "jwt":{"secret":"abababababababababababababababababababababababababababababababab"},
        "scheduler":{"lease_secs":3}
    }))
    .unwrap();
    let state = crate::state::app_state(pool.clone(), settings).unwrap();
    let work = async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let transaction = pool.begin().await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        transaction.commit().await?;
        Ok(())
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        with_lease(&state, id, token, work),
    )
    .await;
    assert!(
        result.is_ok(),
        "lease renewal stopped polling the transaction that owns the only connection"
    );
    result.unwrap().unwrap();
    pool.close().await;
}

#[test]
fn accounting_adapter_keeps_final_fractional_second_and_excludes_next_boundary() {
    let start = chrono::NaiveDate::from_ymd_opt(2026, 9, 30)
        .unwrap()
        .and_hms_opt(17, 0, 0)
        .unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2026, 10, 31)
        .unwrap()
        .and_hms_opt(17, 0, 0)
        .unwrap();
    let range = transaction_range(start, end);
    assert_eq!(range.start.naive_utc(), start);
    assert_eq!(
        range.end.naive_utc(),
        end - chrono::Duration::nanoseconds(1)
    );
    assert!(end - chrono::Duration::nanoseconds(1) <= range.end.naive_utc());
    assert!(end > range.end.naive_utc());
    assert_eq!(range.end.timestamp(), end.and_utc().timestamp() - 1);
}
