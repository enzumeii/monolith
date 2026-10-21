#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    repositories::cron_configurations::tests::{input, state},
    services::report_storage,
};

#[tokio::test]
async fn publication_transaction_failure_recovers_from_saved_bytes_and_acceptance() {
    let (state, directory) = state().await;
    let config = repositories::cron_configurations::insert(&state.pool, &input())
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let first = claim(&state).await.unwrap().unwrap();
    report_storage::prepare(
        &directory,
        first.job.id,
        Utc::now().naive_utc(),
        vec![report_storage::GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: report_storage::CSV_CONTENT_TYPE.into(),
            bytes: b"header\r\noriginal\r\n".to_vec(),
        }],
    )
    .await
    .unwrap();
    report_storage::accept(&directory, first.job.id)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION pg_temp.reject_completion() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='SUCCESS' THEN RAISE EXCEPTION 'owned final checkpoint failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER reject_completion BEFORE UPDATE ON report_results FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_completion();").execute(&state.pool).await.unwrap();
    assert!(execute(&state, &first).await.is_err());
    let status: (String, Option<NaiveDateTime>) =
        sqlx::query_as("SELECT status,published_at FROM cron_jobs")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(status, ("PENDING".into(), None));
    assert!(
        directory
            .join("public")
            .join(first.job.id.to_string())
            .join("inventory.csv")
            .exists()
    );
    sqlx::query("DROP TRIGGER reject_completion ON report_results")
        .execute(&state.pool)
        .await
        .unwrap();
    fail(&state, &first, "Publication checkpoint failed")
        .await
        .unwrap();
    sqlx::query("UPDATE cron_jobs SET lease_until=(CURRENT_TIMESTAMP AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    let retry = claim(&state).await.unwrap().unwrap();
    assert!(execute(&state, &retry).await.is_ok());
    let audit = repositories::report_results::find(&state.pool, first.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.status, "SUCCESS");
    assert_eq!(audit.files.0.len(), 1);
    assert_eq!(
        report_storage::load(&directory, first.job.id)
            .await
            .unwrap()
            .unwrap()
            .files[0]
            .bytes,
        b"header\r\noriginal\r\n"
    );
    state.pool.close().await;
    tokio::fs::remove_dir_all(directory).await.unwrap();
}
