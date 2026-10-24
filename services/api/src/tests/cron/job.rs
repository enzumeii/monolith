#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::tests::Reply;
use crate::{
    client::s3::S3Client,
    models::cron::checkpoint::FileCheckpoint,
    repositories::cron_configurations::tests::{input, state},
    s3_fixture,
};
use chrono::SubsecRound;
use serde_json::json;
use std::sync::Arc;

async fn fixture(
    transaction: bool,
) -> (
    AppState,
    ClaimedJob,
    s3_fixture::Fixture,
    ExecutionCheckpoint,
) {
    let mut state = state().await;
    let blob = s3_fixture::Fixture::new().await;
    state.s3 = Ok(Some(Arc::new(
        S3Client::new(&s3_fixture::Fixture::config(&blob.base)).unwrap(),
    )));
    let mut configuration = input();
    if transaction {
        configuration.report_type = domain::dto::ScheduleReportType::Transaction;
        configuration.parameter = "PREVIOUS_MONTH".into();
    }
    let config = repositories::cron_configurations::insert(&state.pool, &configuration)
        .await
        .unwrap();
    repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = claim(&state).await.unwrap().unwrap();
    assert!(
        repositories::cron_jobs::checkpoint(&state.pool, claim.job.id, claim.token)
            .await
            .unwrap()
            .is_none()
    );
    let generated_at = Utc::now().naive_utc().trunc_subsecs(6);
    let kind = configuration.report_type;
    let prefix = report_service::storage_prefix(kind, generated_at, claim.job.id);
    let files = if transaction {
        vec!["transactions.csv", "items.csv"]
    } else {
        vec!["inventory.csv"]
    };
    let checkpoint = ExecutionCheckpoint {
        generated_at,
        storage_prefix: prefix.clone(),
        files: files
            .into_iter()
            .map(|filename| FileCheckpoint {
                filename: filename.into(),
                key: format!("{prefix}/{}/{filename}", publication::digest(b"original")),
                content_type: CSV_CONTENT_TYPE.into(),
                size_bytes: 8,
                sha256: publication::digest(b"original"),
                receipt: None,
            })
            .collect(),
        mail: MailState::NotStarted,
    };
    persist(&state, &claim, None, &checkpoint).await.unwrap();
    (state, claim, blob, checkpoint)
}
async fn audit(state: &AppState, id: Uuid) -> crate::models::cron::report::ReportResult {
    repositories::report_results::find(&state.pool, id)
        .await
        .unwrap()
        .unwrap()
}
fn populate(blob: &s3_fixture::Fixture, checkpoint: &ExecutionCheckpoint) {
    for file in &checkpoint.files {
        blob.data
            .lock()
            .unwrap()
            .objects
            .insert(("reports".into(), file.key.clone()), b"original".to_vec());
    }
}
async fn mail(
    state: &mut AppState,
    responses: Vec<crate::client::tests::Reply>,
) -> crate::client::tests::Fixture {
    let fixture = crate::client::tests::Fixture::new(responses).await;
    state.resend = Ok(Some(Arc::new(
        crate::client::resend::tests::client_with_url(fixture.url.clone()),
    )));
    fixture
}
#[tokio::test]
async fn partial_two_file_recovery_keeps_available_receipt_and_never_generates_or_emails() {
    let (state, claim, blob, checkpoint) = fixture(true).await;
    blob.data.lock().unwrap().objects.insert(
        ("reports".into(), checkpoint.files[0].key.clone()),
        b"original".to_vec(),
    );
    let failure = execute(&state, &claim).await.err().unwrap();
    assert!(failure.message.contains("publication"));
    assert!(
        failure
            .error
            .to_string()
            .contains("regeneration is disabled")
    );
    fail(&state, &claim, failure.message).await.unwrap();
    let audit = audit(&state, claim.job.id).await;
    assert_eq!(audit.status, "FAILED");
    assert_eq!(audit.files.len(), 1);
    assert_eq!(audit.generated_at, Some(checkpoint.generated_at));
    assert_eq!(blob.data.lock().unwrap().puts.len(), 0);
    let public = serde_json::to_value(audit.into_dto().unwrap()).unwrap();
    for private in [
        "execution_checkpoint",
        "mail",
        "from",
        "storage_prefix",
        "sha256",
    ] {
        assert!(public.get(private).is_none());
    }
    state.pool.close().await;
}
#[tokio::test]
async fn all_files_recovery_publishes_before_mail_and_email_failure_preserves_receipts_and_frozen_intent()
 {
    let (mut state, claim, blob, checkpoint) = fixture(true).await;
    assert!(state.settings.shipping_sender.is_none());
    populate(&blob, &checkpoint);
    let upstream = mail(
        &mut state,
        vec![Reply::status(503), Reply::json(json!({"id":"accepted"}))],
    )
    .await;
    let failure = execute(&state, &claim).await.err().unwrap();
    fail(&state, &claim, failure.message).await.unwrap();
    let failed = audit(&state, claim.job.id).await;
    assert_eq!(failed.files.len(), 2);
    assert_eq!(failed.status, "FAILED");
    sqlx::raw_sql("UPDATE cron_configurations SET receiver_email='changed@example.test', parameter='FIRST_HALF'; UPDATE cron_jobs SET lease_until=(clock_timestamp() AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    let retry = super::claim(&state).await.unwrap().unwrap();
    assert!(execute(&state, &retry).await.is_ok());
    let completed = audit(&state, claim.job.id).await;
    assert_eq!(completed.status, "SUCCESS");
    assert_eq!(completed.files, failed.files);
    let requests = upstream.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].2, requests[1].2);
    assert_eq!(
        requests[0].1["idempotency-key"],
        requests[1].1["idempotency-key"]
    );
    assert_eq!(requests[0].2["from"], "reporting@enzumeii.com");
    assert_eq!(requests[0].2["to"][0], "reports@example.test");
    assert!(
        requests[0].2["subject"]
            .as_str()
            .unwrap()
            .contains("Asia/Bangkok")
    );
    let generated_at = checkpoint
        .generated_at
        .and_utc()
        .with_timezone(&chrono_tz::Asia::Bangkok)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string();
    assert_eq!(failed.generated_at, Some(checkpoint.generated_at));
    assert!(
        requests[0].2["subject"]
            .as_str()
            .unwrap()
            .contains(&format!("Generated: {generated_at} (Asia/Bangkok)"))
    );
    assert!(requests[0].2["html"].as_str().unwrap().contains(&format!(
        "Report generation date: {generated_at} (Asia/Bangkok)"
    )));
    assert_eq!(blob.data.lock().unwrap().puts.len(), 0);
    state.pool.close().await;
}
#[tokio::test]
async fn accepted_checkpoint_skips_sending_and_legacy_intent_stays_immutable() {
    let (mut state, claim, blob, checkpoint) = fixture(false).await;
    populate(&blob, &checkpoint);
    let upstream = mail(
        &mut state,
        vec![crate::client::tests::Reply::json(json!({"id":"accepted"}))],
    )
    .await;
    let mut next = checkpoint.clone();
    let entry = &checkpoint.files[0];
    next.files[0].receipt = Some(crate::models::cron::report::SavedFile {
        filename: entry.filename.clone(),
        content_type: entry.content_type.clone(),
        storage_key: entry.key.clone(),
        bucket: "reports".into(),
        size_bytes: entry.size_bytes,
        sha256: entry.sha256.clone(),
    });
    persist(&state, &claim, Some(&checkpoint), &next)
        .await
        .unwrap();
    let receipts = next.clone();
    let intent = MailIntent {
        from: "old@example.test".into(),
        to: "old-recipient@example.test".into(),
        subject: "Original subject".into(),
        html: "<p>Original body</p>".into(),
        first_attempt_at: Utc::now().naive_utc(),
    };
    next.mail = MailState::Sending(intent.clone());
    persist(&state, &claim, Some(&receipts), &next)
        .await
        .unwrap();
    let mut changed = next.clone();
    if let MailState::Sending(intent) = &mut changed.mail {
        intent.subject = "Changed payload".into();
    }
    assert!(
        persist(&state, &claim, Some(&next), &changed)
            .await
            .is_err()
    );
    assert!(execute(&state, &claim).await.is_ok());
    let requests = upstream.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].2["from"], intent.from);
    assert_eq!(requests[0].2["html"], intent.html);
    // Simulate a crash after accepted checkpoint but before final success commit.
    sqlx::query("UPDATE cron_jobs SET status='PENDING', lease_token=$2, lease_until=(clock_timestamp() AT TIME ZONE 'UTC')+INTERVAL '5 seconds'").bind(claim.job.id).bind(claim.token).execute(&state.pool).await.unwrap();
    assert!(execute(&state, &claim).await.is_ok());
    assert_eq!(upstream.requests().len(), 1);
    state.pool.close().await;
}
#[tokio::test]
async fn missing_checkpoint_with_generated_audit_and_corrupt_checkpoint_fail_closed() {
    let (state, claim, blob, _) = fixture(false).await;
    sqlx::query("UPDATE cron_jobs SET execution_checkpoint=NULL")
        .execute(&state.pool)
        .await
        .unwrap();
    assert!(
        execute(&state, &claim)
            .await
            .err()
            .unwrap()
            .error
            .to_string()
            .contains("regeneration is disabled")
    );
    sqlx::query("UPDATE cron_jobs SET execution_checkpoint=$1")
        .bind(json!({"mail":{"state":"sending"}}))
        .execute(&state.pool)
        .await
        .unwrap();
    assert!(execute(&state, &claim).await.is_err());
    assert_eq!(blob.data.lock().unwrap().puts.len(), 0);
    state.pool.close().await;
}
#[tokio::test]
async fn checkpoint_compare_and_update_fences_stale_expired_tokens_and_immutable_fields() {
    let (state, claim, _, checkpoint) = fixture(false).await;
    let json = serde_json::to_value(&checkpoint).unwrap();
    assert!(json["mail"].get("intent").is_none());
    assert!(!json.to_string().contains("original"));
    for mutation in ["key", "hash", "date", "prefix"] {
        let mut next = checkpoint.clone();
        match mutation {
            "key" => next.files[0].key.push_str("wrong"),
            "hash" => next.files[0].sha256 = "a".repeat(64),
            "date" => next.generated_at += chrono::Duration::days(1),
            _ => next.storage_prefix.push_str("wrong"),
        }
        assert!(
            persist(&state, &claim, Some(&checkpoint), &next)
                .await
                .is_err()
        );
    }
    assert!(persist(&state, &claim, None, &checkpoint).await.is_err());
    let stale_claim = ClaimedJob {
        job: Job {
            id: claim.job.id,
            scheduled_for: claim.job.scheduled_for,
            attempts: claim.job.attempts,
        },
        token: Uuid::new_v4(),
        recipient: claim.recipient.clone(),
    };
    assert!(
        persist(&state, &stale_claim, Some(&checkpoint), &checkpoint)
            .await
            .is_err()
    );
    sqlx::query("UPDATE cron_jobs SET lease_until=(clock_timestamp() AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
    assert!(
        persist(&state, &claim, Some(&checkpoint), &checkpoint)
            .await
            .is_err()
    );
    assert!(
        repositories::cron_jobs::checkpoint(&state.pool, claim.job.id, claim.token)
            .await
            .is_err()
    );
    state.pool.close().await;
}
#[tokio::test]
async fn audit_failure_rolls_back_checkpoint_receipt_and_expired_mail_intent_never_sends() {
    let (mut state, claim, blob, checkpoint) = fixture(false).await;
    populate(&blob, &checkpoint);
    let upstream = mail(&mut state, vec![]).await;
    let mut next = checkpoint.clone();
    let entry = &next.files[0];
    next.files[0].receipt = Some(crate::models::cron::report::SavedFile {
        filename: entry.filename.clone(),
        content_type: entry.content_type.clone(),
        storage_key: entry.key.clone(),
        bucket: "reports".into(),
        size_bytes: entry.size_bytes,
        sha256: entry.sha256.clone(),
    });
    sqlx::raw_sql("CREATE FUNCTION pg_temp.reject_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned failure'; END $$; CREATE TRIGGER reject_receipt BEFORE INSERT ON report_result_files FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_receipt();").execute(&state.pool).await.unwrap();
    assert!(
        persist(&state, &claim, Some(&checkpoint), &next)
            .await
            .is_err()
    );
    assert_eq!(
        repositories::cron_jobs::checkpoint(&state.pool, claim.job.id, claim.token)
            .await
            .unwrap()
            .unwrap(),
        checkpoint
    );
    assert_eq!(
        audit(&state, claim.job.id).await.files,
        [] as [crate::models::cron::report::SavedFile; 0]
    );
    sqlx::query("DROP TRIGGER reject_receipt ON report_result_files")
        .execute(&state.pool)
        .await
        .unwrap();
    persist(&state, &claim, Some(&checkpoint), &next)
        .await
        .unwrap();
    let previous = next.clone();
    next.mail = MailState::Sending(MailIntent {
        from: "old@example.test".into(),
        to: "old@example.test".into(),
        subject: "frozen".into(),
        html: "frozen".into(),
        first_attempt_at: Utc::now().naive_utc() - chrono::Duration::hours(25),
    });
    persist(&state, &claim, Some(&previous), &next)
        .await
        .unwrap();
    assert!(
        execute(&state, &claim)
            .await
            .err()
            .unwrap()
            .error
            .to_string()
            .contains("retry window expired")
    );
    assert_eq!(upstream.requests().len(), 0);
    state.pool.close().await;
}

#[tokio::test]
async fn transaction_start_before_expiry_does_not_extend_checkpoint_lease() {
    let (state, claim, _, checkpoint) = fixture(false).await;
    sqlx::query("UPDATE cron_jobs SET lease_until=(clock_timestamp() AT TIME ZONE 'UTC')+INTERVAL '100 milliseconds'").execute(&state.pool).await.unwrap();
    let mut transaction = state.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_sleep(0.2)")
        .execute(&mut *transaction)
        .await
        .unwrap();
    assert!(
        !repositories::cron_jobs::save_checkpoint(
            &mut transaction,
            claim.job.id,
            claim.token,
            Some(&checkpoint),
            &checkpoint
        )
        .await
        .unwrap()
    );
    transaction.rollback().await.unwrap();
    state.pool.close().await;
}

#[tokio::test]
async fn generated_checkpoint_matches_database_timestamp_precision() {
    use chrono::Timelike;
    let (state, claim, _, _) = fixture(false).await;
    sqlx::raw_sql("UPDATE cron_jobs SET execution_checkpoint=NULL; UPDATE report_results SET generated_at=NULL").execute(&state.pool).await.unwrap();
    let mut prepared = report::prepare(
        claim.job.id,
        domain::dto::ScheduleReportType::Inventory,
        report_service::ReportData::Inventory(vec![]),
        1024,
    )
    .await
    .unwrap();
    assert_eq!(prepared.checkpoint.generated_at.nanosecond() % 1000, 0);
    prepared.checkpoint.generated_at = prepared
        .checkpoint
        .generated_at
        .with_nanosecond(123_456_789)
        .unwrap();
    assert!(prepared.checkpoint.validate().is_err());
    prepared.checkpoint.generated_at = prepared.checkpoint.generated_at.trunc_subsecs(6);
    assert_eq!(prepared.checkpoint.generated_at.nanosecond(), 123_456_000);
    persist(&state, &claim, None, &prepared.checkpoint)
        .await
        .unwrap();
    let saved = audit(&state, claim.job.id).await;
    assert_eq!(saved.generated_at, Some(prepared.checkpoint.generated_at));
    assert!(prepare_report(&state, &claim, &saved).await.is_ok());
    state.pool.close().await;
}

#[tokio::test]
async fn later_file_receipt_survives_an_earlier_missing_file() {
    let (state, claim, s3, checkpoint) = fixture(true).await;
    s3.data.lock().unwrap().objects.insert(
        ("reports".into(), checkpoint.files[1].key.clone()),
        b"original".to_vec(),
    );
    let failure = execute(&state, &claim).await.err().unwrap();
    assert!(failure.message.contains("publication"));
    let saved = audit(&state, claim.job.id).await;
    assert_eq!(saved.files.len(), 1);
    assert_eq!(saved.files[0].filename, "items.csv");
    let position: i32 =
        sqlx::query_scalar("SELECT position FROM report_result_files WHERE report_result_id=$1")
            .bind(claim.job.id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(position, 1);
    assert_eq!(saved.generated_at, Some(checkpoint.generated_at));
    assert_eq!(s3.data.lock().unwrap().puts, [] as [(String, String); 0]);
    state.pool.close().await;
}

#[tokio::test]
async fn corrupt_saved_object_blocks_mail_without_repairing_or_replacing_the_receipt() {
    let (mut state, claim, s3, mut checkpoint) = fixture(false).await;
    populate(&s3, &checkpoint);
    publish_report(&state, &claim, &mut checkpoint, &[])
        .await
        .unwrap();
    let before = audit(&state, claim.job.id).await.files;
    s3.data.lock().unwrap().objects.insert(
        ("reports".into(), checkpoint.files[0].key.clone()),
        b"changed!".to_vec(),
    );
    let upstream = mail(&mut state, vec![]).await;
    let failure = execute(&state, &claim).await.err().unwrap();
    assert!(failure.message.contains("publication"));
    assert_eq!(audit(&state, claim.job.id).await.files, before);
    assert_eq!(
        upstream.requests(),
        [] as [(
            std::string::String,
            reqwest::header::HeaderMap,
            serde_json::Value
        ); 0]
    );
    assert_eq!(s3.data.lock().unwrap().puts, [] as [(String, String); 0]);
    state.pool.close().await;
}

#[tokio::test]
async fn accepted_upload_after_lease_loss_is_recovered_only_by_the_new_worker() {
    let (state, claim, s3, mut old_checkpoint) = fixture(false).await;
    let gate = std::sync::Arc::new(crate::s3_fixture::PutGate::default());
    s3.data.lock().unwrap().put_gate = Some(gate.clone());
    let originals = vec![publication::GeneratedFile {
        filename: "inventory.csv".into(),
        content_type: CSV_CONTENT_TYPE.into(),
        bytes: b"original".to_vec(),
    }];
    let old = publish_report(&state, &claim, &mut old_checkpoint, &originals);
    let new = async {
        gate.started.acquire().await.unwrap().forget();
        sqlx::query("UPDATE cron_jobs SET lease_until=(clock_timestamp() AT TIME ZONE 'UTC')-INTERVAL '1 second'").execute(&state.pool).await.unwrap();
        let retry = super::claim(&state).await.unwrap().unwrap();
        assert_ne!(retry.token, claim.token);
        let mut checkpoint =
            repositories::cron_jobs::checkpoint(&state.pool, retry.job.id, retry.token)
                .await
                .unwrap()
                .unwrap();
        gate.release.add_permits(1);
        gate.accepted.acquire().await.unwrap().forget();
        let files = publish_report(&state, &retry, &mut checkpoint, &[])
            .await
            .unwrap();
        assert_eq!(files, [("inventory.csv".into(), b"original".to_vec())]);
        let saved = audit(&state, retry.job.id).await;
        assert_eq!(saved.files, checkpoint.receipts());
        assert_eq!(saved.files.len(), 1);
    };
    let (old, ()) = tokio::time::timeout(std::time::Duration::from_secs(4), async {
        tokio::join!(old, new)
    })
    .await
    .unwrap();
    assert!(old.unwrap_err().to_string().contains("lease"));
    assert_eq!(s3.data.lock().unwrap().puts.len(), 1);
    state.pool.close().await;
}

#[tokio::test]
async fn conflicting_canonical_receipt_rolls_back_checkpoint_and_generation_date() {
    let (state, claim, _, checkpoint) = fixture(false).await;
    let mut next = checkpoint.clone();
    let file = &next.files[0];
    let receipt = crate::models::cron::report::SavedFile {
        filename: file.filename.clone(),
        content_type: file.content_type.clone(),
        bucket: "reports".into(),
        storage_key: file.key.clone(),
        size_bytes: file.size_bytes,
        sha256: file.sha256.clone(),
    };
    let mut conflicting = receipt.clone();
    conflicting.size_bytes += 1;
    repositories::report_result_files::insert(
        &mut state.pool.acquire().await.unwrap(),
        claim.job.id,
        0,
        &conflicting,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE report_results SET generated_at=NULL WHERE id=$1")
        .bind(claim.job.id)
        .execute(&state.pool)
        .await
        .unwrap();
    next.files[0].receipt = Some(receipt);
    assert!(
        persist(&state, &claim, Some(&checkpoint), &next)
            .await
            .is_err()
    );
    assert_eq!(
        repositories::cron_jobs::checkpoint(&state.pool, claim.job.id, claim.token)
            .await
            .unwrap()
            .unwrap(),
        checkpoint
    );
    let saved = audit(&state, claim.job.id).await;
    assert_eq!(saved.generated_at, None);
    assert_eq!(saved.files, [conflicting]);
    state.pool.close().await;
}
