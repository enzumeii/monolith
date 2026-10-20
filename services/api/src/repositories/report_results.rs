use crate::{
    error::AppError,
    models::report_results::{ReportResult, ResultFilter, SavedFile},
};
use domain::dto::ScheduleReportType;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, types::Json};
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ResultFilter,
) -> Result<Vec<ReportResult>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        r"
        SELECT
            id, report_type, attempted_at, generated_at,
            generation_status, email_status, generation_error, email_error,
            period_start, period_end_exclusive, inventory_id, files
        FROM report_results
        WHERE TRUE
    ",
    );
    if let Some(kind) = filter.report_type {
        query.push(" AND report_type = ").push_bind(match kind {
            ScheduleReportType::Inventory => "INVENTORY",
            ScheduleReportType::Transaction => "TRANSACTION",
        });
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
    Ok(sqlx::query_as(
        r"
        SELECT
            id, report_type, attempted_at, generated_at,
            generation_status, email_status, generation_error, email_error,
            period_start, period_end_exclusive, inventory_id, files
        FROM report_results
        WHERE id = $1
    ",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn save_files(
    connection: &mut PgConnection,
    id: Uuid,
    files: &[SavedFile],
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        UPDATE report_results
        SET files = $2
        WHERE id = $1
    ",
    )
    .bind(id)
    .bind(Json(files))
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}
