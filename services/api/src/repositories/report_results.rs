use crate::{
    error::AppError,
    models::{
        cron::report::{ReportResult, ResultFilter, SavedFile},
        cron::report_type_value,
    },
};
use chrono::NaiveDateTime;
use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ResultFilter,
) -> Result<Vec<ReportResult>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        "WITH page AS (SELECT id, report_type, attempted_at, generated_at, status, error, parameters FROM report_results WHERE TRUE",
    );
    if let Some(kind) = filter.report_type {
        query
            .push(" AND report_type = ")
            .push_bind(report_type_value(kind));
    }
    if let Some(from) = filter.generated_from {
        query
            .push(" AND COALESCE(generated_at, attempted_at) >= ")
            .push_bind(from);
    }
    if let Some(to) = filter.generated_to {
        query
            .push(" AND COALESCE(generated_at, attempted_at) < ")
            .push_bind(to);
    }
    query
        .push(" ORDER BY COALESCE(generated_at, attempted_at) DESC, id DESC LIMIT ")
        .push_bind(i64::from(filter.per_page) + 1)
        .push(" OFFSET ")
        .push_bind(i64::from(filter.page - 1) * i64::from(filter.per_page));
    query.push(") ").push(JOINED_PAGE);
    collect(query.build_query_as().fetch_all(pool).await?)
}
pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<ReportResult>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        "WITH page AS (SELECT id, report_type, attempted_at, generated_at, status, error, parameters FROM report_results WHERE id=",
    );
    query.push_bind(id).push(") ").push(JOINED_PAGE);
    Ok(collect(query.build_query_as().fetch_all(pool).await?)?.pop())
}
pub(crate) async fn start(
    connection: &mut PgConnection,
    id: Uuid,
    kind: &str,
    parameters: &str,
    now: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query(
        r"
        INSERT INTO report_results (id, report_type, parameters, attempted_at, status)
        VALUES ($1, $2, $3, $4, 'PENDING')
        ON CONFLICT (id) DO UPDATE SET status = 'PENDING'
    ",
    )
    .bind(id)
    .bind(kind)
    .bind(parameters)
    .bind(now)
    .execute(connection)
    .await?;
    Ok(())
}
pub(crate) async fn fail(
    connection: &mut PgConnection,
    id: Uuid,
    error: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE report_results SET status = 'FAILED', error = $2 WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(connection)
        .await?;
    Ok(())
}
pub(crate) async fn complete(
    connection: &mut PgConnection,
    id: Uuid,
    files: &[SavedFile],
) -> Result<bool, AppError> {
    let stored = super::report_result_files::load(connection, id).await?;
    if stored != files {
        return Err(crate::models::cron::saved_invalid());
    }
    Ok(
        sqlx::query("UPDATE report_results SET status = 'SUCCESS', error = NULL WHERE id = $1")
            .bind(id)
            .execute(connection)
            .await?
            .rows_affected()
            == 1,
    )
}

pub(crate) async fn context(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<(String, String)>, AppError> {
    Ok(
        sqlx::query_as("SELECT report_type, parameters FROM report_results WHERE id=$1")
            .bind(id)
            .fetch_optional(connection)
            .await?,
    )
}

const JOINED_PAGE: &str = "SELECT page.*, file.filename, file.content_type, file.bucket,
    file.storage_key, file.size_bytes, file.sha256 FROM page
    LEFT JOIN report_result_files file ON file.report_result_id=page.id
    ORDER BY COALESCE(page.generated_at,page.attempted_at) DESC, page.id DESC, file.position";

#[derive(FromRow)]
struct JoinedReport {
    id: Uuid,
    report_type: String,
    attempted_at: NaiveDateTime,
    generated_at: Option<NaiveDateTime>,
    status: String,
    error: Option<String>,
    parameters: String,
    filename: Option<String>,
    content_type: Option<String>,
    bucket: Option<String>,
    storage_key: Option<String>,
    size_bytes: Option<i64>,
    sha256: Option<String>,
}

fn collect(rows: Vec<JoinedReport>) -> Result<Vec<ReportResult>, AppError> {
    let mut reports: Vec<ReportResult> = Vec::new();
    for row in rows {
        let file = if let Some(filename) = row.filename {
            Some(SavedFile {
                filename,
                content_type: row
                    .content_type
                    .ok_or_else(crate::models::cron::saved_invalid)?,
                bucket: row.bucket.ok_or_else(crate::models::cron::saved_invalid)?,
                storage_key: row
                    .storage_key
                    .ok_or_else(crate::models::cron::saved_invalid)?,
                size_bytes: usize::try_from(
                    row.size_bytes
                        .ok_or_else(crate::models::cron::saved_invalid)?,
                )
                .map_err(|_| crate::models::cron::saved_invalid())?,
                sha256: row.sha256.ok_or_else(crate::models::cron::saved_invalid)?,
            })
        } else {
            None
        };
        if let Some(report) = reports.last_mut().filter(|report| report.id == row.id) {
            if let Some(file) = file {
                report.files.push(file);
            }
        } else {
            reports.push(ReportResult {
                id: row.id,
                report_type: row.report_type,
                attempted_at: row.attempted_at,
                generated_at: row.generated_at,
                status: row.status,
                error: row.error,
                parameters: row.parameters,
                files: file.into_iter().collect(),
            });
        }
    }
    Ok(reports)
}
