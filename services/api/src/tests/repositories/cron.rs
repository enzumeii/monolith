#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    config::Settings,
    state::{AppState, app_state},
};
use chrono::{Duration, Utc};
use serde_json::json;

#[tokio::test]
async fn queue_checkpoints_eagerly_audit_failures_and_fence_stale_workers() {
    let (state, directory) = isolated_state().await;
    let (configuration, job) = queued_job(&state).await;
    let token = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let claimed = claim_generation(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, job);
    assert!(
        !complete_generation(&state.pool, job, Uuid::new_v4(), now)
            .await
            .unwrap()
    );
    assert!(
        fail_generation(
            &state.pool,
            job,
            token,
            "Report generation failed",
            Some(now)
        )
        .await
        .unwrap()
    );
    let failed: (String, Option<String>) =
        sqlx::query_as("SELECT generation_status,generation_error FROM report_results WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(
        failed,
        ("FAILED".into(), Some("Report generation failed".into()))
    );
    let token = Uuid::new_v4();
    claim_generation(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    assert!(
        complete_generation(&state.pool, job, token, now)
            .await
            .unwrap()
    );
    delete_configuration(&mut state.pool.acquire().await.unwrap(), configuration)
        .await
        .unwrap();
    assert!(
        crate::repositories::report_results::find(&state.pool, job)
            .await
            .unwrap()
            .is_some()
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn email_failures_are_eagerly_audited_and_final_expired_attempts_become_unknown() {
    let (state, directory) = isolated_state().await;
    let job = generated_job(&state).await;
    let now = Utc::now().naive_utc();
    let email_token = Uuid::new_v4();
    claim_email(
        &state.pool,
        email_token,
        now,
        now + Duration::seconds(30),
        2,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        fail_email(
            &state.pool,
            job,
            email_token,
            "Email delivery attempt failed",
            Some(now),
            false
        )
        .await
        .unwrap()
    );
    let email: (String, Option<String>) =
        sqlx::query_as("SELECT email_status,email_error FROM report_results WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(
        email,
        (
            "FAILED".into(),
            Some("Email delivery attempt failed".into())
        )
    );
    let token = Uuid::new_v4();
    claim_email(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=$2 WHERE id=$1")
        .bind(job)
        .bind(now - Duration::seconds(1))
        .execute(&state.pool)
        .await
        .unwrap();
    assert!(
        !complete_email(&state.pool, job, token, "late", now)
            .await
            .unwrap()
    );
    recover_expired(&state.pool, now, 2).await.unwrap();
    let status: String = sqlx::query_scalar("SELECT email_status FROM report_results WHERE id=$1")
        .bind(job)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(status, "UNKNOWN");
    assert!(
        claim_email(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            2
        )
        .await
        .unwrap()
        .is_none()
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn audit_insert_failure_rolls_back_the_job_claim() {
    let (state, directory) = isolated_state().await;
    let (_, job) = queued_job(&state).await;
    reject_audit(&state.pool).await;
    let now = Utc::now().naive_utc();
    assert!(
        claim_generation(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            2
        )
        .await
        .is_err()
    );
    let status: (String, i32) =
        sqlx::query_as("SELECT generation_status,generation_attempts FROM cron_jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(status, ("QUEUED".into(), 0));
    sqlx::query("DROP TRIGGER reject_report_audit ON report_results")
        .execute(&state.pool)
        .await
        .unwrap();
    assert!(
        claim_generation(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            2
        )
        .await
        .unwrap()
        .is_some()
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn file_format_patch_preserves_due_cursor_and_dispatched_snapshot() {
    let (state, directory) = isolated_state().await;
    let (configuration, job) = queued_job(&state).await;
    sqlx::query("UPDATE cron_configurations SET status='ENABLED', next_run_at=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC') + INTERVAL '7 days' WHERE id=$1")
        .bind(configuration).execute(&state.pool).await.unwrap();
    let before = find_configuration(&state.pool, configuration)
        .await
        .unwrap()
        .unwrap()
        .next_run_at;
    assert!(before.is_some());
    let patch = serde_json::from_value(json!({"file_format":"XLSX"})).unwrap();
    let changed = crate::services::cron_configurations::update(&state.pool, configuration, patch)
        .await
        .unwrap();
    assert_eq!(changed.file_format, ReportFileFormat::Xlsx);
    assert_eq!(
        find_configuration(&state.pool, configuration)
            .await
            .unwrap()
            .unwrap()
            .next_run_at,
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT file_format FROM cron_jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        "CSV"
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

async fn reject_audit(pool: &PgPool) {
    sqlx::raw_sql("CREATE FUNCTION pg_temp.reject_report_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned audit fault'; END $$; CREATE TRIGGER reject_report_audit BEFORE INSERT OR UPDATE ON report_results FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_report_audit();")
        .execute(pool).await.unwrap();
}

async fn queued_job(state: &AppState) -> (Uuid, Uuid) {
    let mut connection = state.pool.acquire().await.unwrap();
    let config = insert_configuration(
        &mut connection,
        "owned",
        ScheduleReportType::Inventory,
        "0 9 * * *",
        "fixture@example.test",
        None,
        Some("123"),
        ScheduleStatus::Disabled,
        None,
        ReportFileFormat::Csv,
    )
    .await
    .unwrap();
    insert_queued_result(&mut connection, &config, Utc::now().naive_utc(), None, None)
        .await
        .unwrap();
    let id = sqlx::query_scalar("SELECT id FROM cron_jobs")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    (config.id, id)
}

pub(crate) async fn isolated_state() -> (AppState, std::path::PathBuf) {
    let directory = std::env::temp_dir().join(format!("report-queue-{}", Uuid::new_v4()));
    let settings: Settings = serde_json::from_value(json!({
        "server":{"host":"127.0.0.1","port":0},
        "database":{"url":std::env::var("E2E_DATABASE_URL").unwrap_or_else(|_|"postgres://e2e:e2e@127.0.0.1:5433/e2e".into()),"max_connections":1,"min_connections":1},
        "browser":{"public_origin":"http://127.0.0.1:8084","insecure_local":true},
        "jwt":{"secret":"abababababababababababababababababababababababababababababababab"},
        "reports":{"storage_directory":directory}
    })).unwrap();
    let pool = crate::repositories::create_pool(&settings).await.unwrap();
    sqlx::raw_sql("CREATE TEMP TABLE cron_configurations (LIKE public.cron_configurations INCLUDING ALL); CREATE TEMP TABLE cron_jobs (LIKE public.cron_jobs INCLUDING ALL); CREATE TEMP TABLE report_results (LIKE public.report_results INCLUDING ALL);")
        .execute(&pool).await.unwrap();
    (app_state(pool, settings).unwrap(), directory)
}

pub(crate) async fn generated_job(state: &AppState) -> Uuid {
    let (_, job) = queued_job(state).await;
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    claim_generation(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    assert!(
        complete_generation(&state.pool, job, token, now)
            .await
            .unwrap()
    );
    job
}

#[tokio::test]
async fn ambiguous_email_never_replays_outside_the_provider_idempotency_window() {
    let (state, directory) = isolated_state().await;
    let job = generated_job(&state).await;
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    claim_email(&state.pool, token, now, now + Duration::seconds(30), 5)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=$2,email_first_attempt_at=$3 WHERE id=$1")
        .bind(job)
        .bind(now - Duration::seconds(1))
        .bind(now - Duration::hours(24) - Duration::seconds(1))
        .execute(&state.pool)
        .await
        .unwrap();
    recover_expired(&state.pool, now, 5).await.unwrap();
    assert!(
        claim_email(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            5
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT email_status FROM report_results WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        "UNKNOWN"
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn generation_final_expired_attempt_is_audited_as_failed() {
    let (state, directory) = isolated_state().await;
    let (_, job) = queued_job(&state).await;
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    claim_generation(&state.pool, token, now, now + Duration::seconds(30), 1)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=$2 WHERE id=$1")
        .bind(job)
        .bind(now - Duration::seconds(1))
        .execute(&state.pool)
        .await
        .unwrap();
    recover_expired(&state.pool, now, 1).await.unwrap();
    assert!(
        claim_generation(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            1
        )
        .await
        .unwrap()
        .is_none()
    );
    let failure: (String, Option<String>) =
        sqlx::query_as("SELECT generation_status,generation_error FROM report_results WHERE id=$1")
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(failure.0, "FAILED");
    assert!(failure.1.unwrap().contains("final attempt"));
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn publication_final_expired_attempt_records_failure_without_repeating_email() {
    let (state, directory) = isolated_state().await;
    let id = generated_job(&state).await;
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    claim_email(&state.pool, token, now, now + Duration::seconds(30), 1)
        .await
        .unwrap()
        .unwrap();
    assert!(
        complete_email(&state.pool, id, token, "accepted", now)
            .await
            .unwrap()
    );
    claim_publication(
        &state.pool,
        Uuid::new_v4(),
        now,
        now + Duration::seconds(30),
        1,
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=$2 WHERE id=$1")
        .bind(id)
        .bind(now - Duration::seconds(1))
        .execute(&state.pool)
        .await
        .unwrap();
    recover_expired(&state.pool, now, 1).await.unwrap();
    let terminal: (String,Option<Uuid>,Option<String>,i32) = sqlx::query_as("SELECT email_status,lease_token,publication_error,publication_attempts FROM cron_jobs WHERE id=$1")
        .bind(id).fetch_one(&state.pool).await.unwrap();
    assert_eq!(terminal.0, "SENT");
    assert!(
        terminal.1.is_none(),
        "expired final publisher must release ownership"
    );
    assert!(
        terminal.2.is_some(),
        "expired final publisher must record its failure"
    );
    assert_eq!(terminal.3, 1);
    assert!(
        claim_publication(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            1
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        claim_email(
            &state.pool,
            Uuid::new_v4(),
            now,
            now + Duration::seconds(30),
            1
        )
        .await
        .unwrap()
        .is_none()
    );
    state.pool.close().await;
    let _ = tokio::fs::remove_dir_all(directory).await;
}

#[tokio::test]
async fn audit_commit_failure_preserves_published_bytes_and_email_acceptance() {
    use crate::services::report_storage::{self, CSV_CONTENT_TYPE, GeneratedFile};
    let (state, directory) = isolated_state().await;
    let id = generated_job(&state).await;
    let now = Utc::now().naive_utc();
    let token = Uuid::new_v4();
    claim_email(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    assert!(
        complete_email(&state.pool, id, token, "accepted", now)
            .await
            .unwrap()
    );
    let token = Uuid::new_v4();
    let job = claim_publication(&state.pool, token, now, now + Duration::seconds(30), 2)
        .await
        .unwrap()
        .unwrap();
    let bytes = b"sku,title\r\n42,original\r\n".to_vec();
    report_storage::prepare(
        &directory,
        id,
        now,
        ReportFileFormat::Csv,
        vec![GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: bytes.clone(),
        }],
    )
    .await
    .unwrap();
    let files = report_storage::publish(&directory, id).await.unwrap();
    reject_audit(&state.pool).await;
    let mut transaction = state.pool.begin().await.unwrap();
    assert!(
        finish_publication(&mut transaction, id, Some(token))
            .await
            .unwrap()
    );
    assert!(
        crate::repositories::report_results::save_files(&mut transaction, job.id, &files)
            .await
            .is_err()
    );
    transaction.rollback().await.unwrap();
    let checkpoints: (String, Option<NaiveDateTime>) =
        sqlx::query_as("SELECT email_status,published_at FROM cron_jobs WHERE id=$1")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(checkpoints, ("SENT".into(), None));
    sqlx::query("DROP TRIGGER reject_report_audit ON report_results")
        .execute(&state.pool)
        .await
        .unwrap();
    let files = report_storage::publish(&directory, id).await.unwrap();
    let mut transaction = state.pool.begin().await.unwrap();
    assert!(
        finish_publication(&mut transaction, id, Some(token))
            .await
            .unwrap()
    );
    crate::repositories::report_results::save_files(&mut transaction, job.id, &files)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(
        report_storage::read_published(&directory, id, &files[0])
            .await
            .unwrap()
            .bytes,
        bytes
    );
    state.pool.close().await;
    tokio::fs::remove_dir_all(directory).await.unwrap();
}
