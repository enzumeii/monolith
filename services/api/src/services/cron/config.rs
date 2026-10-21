pub(crate) mod schedule;

use crate::{
    error::AppError,
    models::cron::{ConfigurationFilter, ConfigurationInput, CronConfiguration, timestamp},
    repositories,
};
use axum::http::StatusCode;
use chrono::Utc;
use domain::dto::{
    CronConfigurationDTO, CronConfigurationListResponse, CronConfigurationPreview,
    CronOccurrencePreview, CronPeriodPreview,
};
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) async fn list(
    pool: &PgPool,
    filter: ConfigurationFilter,
) -> Result<CronConfigurationListResponse, AppError> {
    let mut records = repositories::cron_configurations::list(pool, filter).await?;
    let has_next = records.len() > filter.per_page as usize;
    records.truncate(filter.per_page as usize);
    Ok(CronConfigurationListResponse {
        items: records
            .into_iter()
            .map(CronConfiguration::into_dto)
            .collect::<Result<_, _>>()?,
        page: filter.page,
        per_page: filter.per_page,
        has_next,
    })
}
pub(crate) async fn get(pool: &PgPool, id: Uuid) -> Result<CronConfiguration, AppError> {
    repositories::cron_configurations::find(pool, id)
        .await?
        .ok_or_else(missing)
}
pub(crate) async fn create(
    pool: &PgPool,
    input: &ConfigurationInput,
) -> Result<CronConfigurationDTO, AppError> {
    repositories::cron_configurations::insert(pool, input)
        .await?
        .into_dto()
}
pub(crate) async fn update(
    pool: &PgPool,
    id: Uuid,
    observed: chrono::NaiveDateTime,
    input: &ConfigurationInput,
) -> Result<CronConfigurationDTO, AppError> {
    if let Some(record) =
        repositories::cron_configurations::update(pool, id, observed, input).await?
    {
        return record.into_dto();
    }
    get(pool, id).await?;
    Err(AppError::new(
        StatusCode::CONFLICT,
        "Cron configuration changed; reload before saving",
    ))
}
pub(crate) async fn delete(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    repositories::cron_configurations::lock(&mut transaction, id)
        .await?
        .ok_or_else(missing)?;
    repositories::cron_jobs::cancel(&mut transaction, id).await?;
    repositories::cron_configurations::delete(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(())
}
pub(crate) fn preview(
    request: &crate::validations::cron::ValidatedPreview,
) -> Result<CronConfigurationPreview, AppError> {
    let scheduled =
        schedule::next_occurrences(&request.expression, Utc::now(), 5).map_err(schedule_error)?;
    let mut occurrences = Vec::new();
    for due in scheduled {
        let period = if let Some(mode) = request.period {
            let window = schedule::transaction_window(mode, due).map_err(schedule_error)?;
            Some(CronPeriodPreview {
                period_start: timestamp(window.start),
                period_end_exclusive: timestamp(window.end_exclusive),
            })
        } else {
            None
        };
        occurrences.push(CronOccurrencePreview {
            scheduled_for: timestamp(due.naive_utc()),
            period,
        });
    }
    Ok(CronConfigurationPreview {
        schedule_timezone: "Asia/Bangkok".into(),
        description: schedule::describe(&request.expression).map_err(schedule_error)?,
        occurrences,
    })
}
pub(crate) fn schedule_error(_: schedule::CronScheduleError) -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Unable to evaluate saved cron schedule",
    )
}
fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Cron configuration not found")
}
