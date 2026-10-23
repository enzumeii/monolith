#![allow(clippy::unwrap_used)]
use super::*;
use crate::{models::cron::report::ResultFilter, repositories};
use repositories::cron_configurations::tests::state;

async fn parent(pool: &sqlx::PgPool, id: Uuid) {
    sqlx::query("INSERT INTO report_results(id,report_type,attempted_at,generated_at,status,parameters) VALUES ($1,'TRANSACTION','2026-11-01','2026-11-01','FAILED','2026-10-01T00:00:00+07:00 - 2026-10-31T23:59:59+07:00')")
        .bind(id).execute(pool).await.unwrap();
}
fn file(id: Uuid, filename: &str) -> SavedFile {
    SavedFile {
        filename: filename.into(),
        content_type: report_generator::CSV_CONTENT_TYPE.into(),
        bucket: "reports".into(),
        size_bytes: 8,
        sha256: "a".repeat(64),
        storage_key: format!("transaction/20261101/{id}/{}/{filename}", "a".repeat(64)),
    }
}

#[tokio::test]
async fn exact_receipt_idempotence_conflicts_and_database_constraints() {
    let state = state().await;
    let id = Uuid::new_v4();
    let other = Uuid::new_v4();
    parent(&state.pool, id).await;
    parent(&state.pool, other).await;
    let original = file(id, "items.csv");
    let mut connection = state.pool.acquire().await.unwrap();
    insert(&mut connection, id, 1, &original).await.unwrap();
    insert(&mut connection, id, 1, &original).await.unwrap();
    assert!(insert(&mut connection, id, 0, &original).await.is_err());
    let mut conflict = original.clone();
    conflict.size_bytes += 1;
    assert!(insert(&mut connection, id, 1, &conflict).await.is_err());
    assert_eq!(
        load(&mut connection, id).await.unwrap(),
        std::slice::from_ref(&original)
    );
    let mut oversized = original.clone();
    oversized.size_bytes = usize::MAX;
    assert!(insert(&mut connection, id, 1, &oversized).await.is_err());
    assert_constraints(&mut connection, id, other, &original).await;
    drop(connection);
    assert!(
        repositories::report_results::find(&state.pool, id)
            .await
            .unwrap()
            .unwrap()
            .into_dto()
            .is_ok()
    );
    let other_file = file(other, "transactions.csv");
    insert(
        &mut state.pool.acquire().await.unwrap(),
        other,
        0,
        &other_file,
    )
    .await
    .unwrap();
    sqlx::query("DELETE FROM report_results WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await
        .unwrap();
    assert_eq!(
        load(&mut state.pool.acquire().await.unwrap(), id)
            .await
            .unwrap(),
        [] as [SavedFile; 0]
    );
    assert_eq!(
        load(&mut state.pool.acquire().await.unwrap(), other)
            .await
            .unwrap(),
        [other_file]
    );
    let columns: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_attribute WHERE attrelid='report_results'::regclass AND attname='files' AND NOT attisdropped").fetch_one(&state.pool).await.unwrap();
    assert_eq!(columns, 0);
    state.pool.close().await;
}

#[tokio::test]
async fn paged_parents_keep_two_files_in_frozen_order_and_empty_reports() {
    let state = state().await;
    let mut ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    ids.sort_unstable();
    for id in ids {
        parent(&state.pool, id).await;
    }
    let id = ids[2];
    let items = file(id, "items.csv");
    let transactions = file(id, "transactions.csv");
    insert(&mut state.pool.acquire().await.unwrap(), id, 1, &items)
        .await
        .unwrap();
    assert_eq!(
        repositories::report_results::find(&state.pool, id)
            .await
            .unwrap()
            .unwrap()
            .files,
        std::slice::from_ref(&items)
    );
    insert(
        &mut state.pool.acquire().await.unwrap(),
        id,
        0,
        &transactions,
    )
    .await
    .unwrap();
    let filter = ResultFilter {
        report_type: None,
        generated_from: None,
        generated_to: None,
        page: 1,
        per_page: 1,
    };
    let reports = repositories::report_results::list(&state.pool, filter)
        .await
        .unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].id, id);
    assert_eq!(reports[0].files, [transactions.clone(), items.clone()]);
    assert_eq!(reports[1].files, [] as [SavedFile; 0]);
    let page = crate::services::cron::report::list(&state.pool, filter)
        .await
        .unwrap();
    assert!(page.has_next);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].files.len(), 2);
    let last = crate::services::cron::report::list(&state.pool, ResultFilter { page: 3, ..filter })
        .await
        .unwrap();
    assert!(!last.has_next);
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].files, [] as [domain::dto::ReportFileDTO; 0]);
    let mut transaction = state.pool.begin().await.unwrap();
    assert!(
        repositories::report_results::complete(&mut transaction, id, &[])
            .await
            .is_err()
    );
    transaction.rollback().await.unwrap();
    let mut transaction = state.pool.begin().await.unwrap();
    assert!(
        repositories::report_results::complete(&mut transaction, id, &[transactions, items])
            .await
            .unwrap()
    );
    transaction.commit().await.unwrap();
    assert_eq!(
        repositories::report_results::find(&state.pool, id)
            .await
            .unwrap()
            .unwrap()
            .status,
        "SUCCESS"
    );
    state.pool.close().await;
}

async fn assert_constraints(
    connection: &mut PgConnection,
    id: Uuid,
    other: Uuid,
    original: &SavedFile,
) {
    for (position, size, hash, bucket, key, parent_id, name) in [
        (
            -1,
            8,
            "a".repeat(64),
            "reports",
            "negative",
            id,
            "negative.csv",
        ),
        (0, -1, "a".repeat(64), "reports", "size", id, "size.csv"),
        (0, 8, "A".repeat(64), "reports", "hash", id, "hash.csv"),
        (
            0,
            8,
            "a".repeat(63),
            "reports",
            "hash-length",
            id,
            "hash.csv",
        ),
        (0, 8, "a".repeat(64), "", "bucket", id, "bucket.csv"),
        (
            1,
            8,
            "a".repeat(64),
            "reports",
            "duplicate-position",
            id,
            "other.csv",
        ),
        (
            0,
            8,
            "a".repeat(64),
            "reports",
            original.storage_key.as_str(),
            other,
            "items.csv",
        ),
        (
            0,
            8,
            "a".repeat(64),
            "reports",
            "orphan",
            Uuid::new_v4(),
            "orphan.csv",
        ),
        (0, 8, "a".repeat(64), "reports", "empty-name", id, ""),
        (0, 8, "a".repeat(64), "reports", "", id, "empty-key.csv"),
    ] {
        assert!(sqlx::query("INSERT INTO report_result_files(report_result_id,filename,position,content_type,bucket,storage_key,size_bytes,sha256) VALUES ($1,$2,$3,'text/csv; charset=utf-8',$4,$5,$6,$7)")
            .bind(parent_id).bind(name).bind(position).bind(bucket).bind(key).bind(size).bind(hash).execute(&mut *connection).await.is_err(),"{name}/{key}");
    }
    assert!(sqlx::query("INSERT INTO report_result_files(report_result_id,filename,position,content_type,bucket,storage_key,size_bytes,sha256) VALUES ($1,'empty-type.csv',0,'','reports','empty-type',8,$2)")
        .bind(id).bind("a".repeat(64)).execute(&mut *connection).await.is_err());
}
