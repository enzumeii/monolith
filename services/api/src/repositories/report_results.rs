use crate::{
    error::AppError,
    models::{
        cron::report_type_value,
        report_results::{ReportResult, ResultFilter, SavedFile},
    },
};
use chrono::NaiveDateTime;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, types::Json};
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ResultFilter,
) -> Result<Vec<ReportResult>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM report_results WHERE TRUE");
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
    Ok(query.build_query_as().fetch_all(pool).await?)
}
pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<ReportResult>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM report_results WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
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
pub(crate) async fn generated(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    at: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        WITH owned AS (
            SELECT id FROM cron_jobs WHERE id = $1 AND lease_token = $2
                AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC') FOR UPDATE
        )
        UPDATE report_results SET generated_at = $3 WHERE id IN (SELECT id FROM owned)
    ",
    )
    .bind(id)
    .bind(token)
    .bind(at)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
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
    Ok(sqlx::query(
        "UPDATE report_results SET status = 'SUCCESS', error = NULL, files = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(Json(files))
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
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
