use crate::{error::AppError, models::cron::report::SavedFile};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

pub(crate) async fn insert(
    connection: &mut PgConnection,
    report_id: Uuid,
    position: i32,
    file: &SavedFile,
) -> Result<(), AppError> {
    crate::validations::cron::report::validate_saved_file(file)?;
    let size = i64::try_from(file.size_bytes).map_err(|_| crate::models::cron::saved_invalid())?;
    let inserted: Option<Uuid> = sqlx::query_scalar(
        r"INSERT INTO report_result_files
            (report_result_id, filename, position, content_type, bucket, storage_key, size_bytes, sha256)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
        ON CONFLICT (report_result_id, filename) DO UPDATE SET filename=EXCLUDED.filename
        WHERE report_result_files.position=EXCLUDED.position
            AND report_result_files.content_type=EXCLUDED.content_type
            AND report_result_files.bucket=EXCLUDED.bucket
            AND report_result_files.storage_key=EXCLUDED.storage_key
            AND report_result_files.size_bytes=EXCLUDED.size_bytes
            AND report_result_files.sha256=EXCLUDED.sha256
        RETURNING report_result_id",
    )
    .bind(report_id)
    .bind(&file.filename)
    .bind(position)
    .bind(&file.content_type)
    .bind(&file.bucket)
    .bind(&file.storage_key)
    .bind(size)
    .bind(&file.sha256)
    .fetch_optional(connection)
    .await?;
    if inserted.is_none() {
        return Err(crate::models::cron::saved_invalid());
    }
    Ok(())
}

pub(crate) async fn load(
    connection: &mut PgConnection,
    report_id: Uuid,
) -> Result<Vec<SavedFile>, AppError> {
    let rows: Vec<ReceiptRow> = sqlx::query_as("SELECT filename,content_type,bucket,storage_key,size_bytes,sha256 FROM report_result_files WHERE report_result_id=$1 ORDER BY position")
        .bind(report_id).fetch_all(connection).await?;
    rows.into_iter()
        .map(|row| {
            Ok(SavedFile {
                filename: row.filename,
                content_type: row.content_type,
                bucket: row.bucket,
                storage_key: row.storage_key,
                sha256: row.sha256,
                size_bytes: usize::try_from(row.size_bytes)
                    .map_err(|_| crate::models::cron::saved_invalid())?,
            })
        })
        .collect()
}

#[derive(FromRow)]
struct ReceiptRow {
    filename: String,
    content_type: String,
    bucket: String,
    storage_key: String,
    size_bytes: i64,
    sha256: String,
}

#[cfg(test)]
#[path = "../tests/repositories/report_result_files.rs"]
mod tests;
