use crate::{
    error::AppError,
    models::cron::{ConfigurationFilter, ConfigurationInput, CronConfiguration, report_type_value},
};
use chrono::NaiveDateTime;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ConfigurationFilter,
) -> Result<Vec<CronConfiguration>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM cron_configurations WHERE TRUE");
    if let Some(kind) = filter.report_type {
        query
            .push(" AND report_type = ")
            .push_bind(report_type_value(kind));
    }
    if let Some(enabled) = filter.enabled {
        query.push(" AND enabled = ").push_bind(enabled);
    }
    query
        .push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(i64::from(filter.per_page) + 1)
        .push(" OFFSET ")
        .push_bind(i64::from(filter.page - 1) * i64::from(filter.per_page));
    Ok(query.build_query_as().fetch_all(pool).await?)
}
pub(crate) async fn find(pool: &PgPool, id: Uuid) -> Result<Option<CronConfiguration>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM cron_configurations WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}
pub(crate) async fn insert(
    pool: &PgPool,
    input: &ConfigurationInput,
) -> Result<CronConfiguration, AppError> {
    Ok(sqlx::query_as(r"
        INSERT INTO cron_configurations (name, report_type, cron_expression, receiver_email, parameter, enabled)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
    ").bind(&input.name).bind(report_type_value(input.report_type)).bind(&input.cron_expression)
        .bind(&input.receiver_email).bind(&input.parameter).bind(input.enabled).fetch_one(pool).await?)
}
pub(crate) async fn update(
    pool: &PgPool,
    id: Uuid,
    observed: NaiveDateTime,
    input: &ConfigurationInput,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(r"
        UPDATE cron_configurations
        SET name = $3, report_type = $4, cron_expression = $5, receiver_email = $6,
            parameter = $7, enabled = $8,
            updated_at = GREATEST(CURRENT_TIMESTAMP AT TIME ZONE 'UTC', updated_at + INTERVAL '1 microsecond')
        WHERE id = $1 AND updated_at = $2
        RETURNING *
    ").bind(id).bind(observed).bind(&input.name).bind(report_type_value(input.report_type))
        .bind(&input.cron_expression).bind(&input.receiver_email).bind(&input.parameter).bind(input.enabled)
        .fetch_optional(pool).await?)
}
pub(crate) async fn lock(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM cron_configurations WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(connection)
            .await?,
    )
}
pub(crate) async fn enabled_page(
    pool: &PgPool,
    after: Option<Uuid>,
) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        r"
        SELECT id FROM cron_configurations
        WHERE enabled AND ($1::uuid IS NULL OR id > $1)
        ORDER BY id LIMIT 50
    ",
    )
    .bind(after)
    .fetch_all(pool)
    .await?)
}
pub(crate) async fn delete(connection: &mut PgConnection, id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM cron_configurations WHERE id = $1")
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}

pub(crate) async fn try_lock(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM cron_configurations WHERE id=$1 FOR UPDATE SKIP LOCKED")
            .bind(id)
            .fetch_optional(connection)
            .await?,
    )
}

#[cfg(test)]
#[path = "../tests/repositories/cron_configurations.rs"]
pub(crate) mod tests;
