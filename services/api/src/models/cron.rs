pub(crate) mod checkpoint;
pub(crate) mod configs;
pub(crate) mod report;
pub(crate) use configs::{ConfigurationFilter, ConfigurationInput, CronConfiguration};

use crate::error::AppError;
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use domain::dto::ScheduleReportType;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct Job {
    pub id: Uuid,
    pub scheduled_for: NaiveDateTime,
    pub attempts: i32,
}
pub(crate) fn report_type(value: &str) -> Result<ScheduleReportType, AppError> {
    match value {
        "TRANSACTION" => Ok(ScheduleReportType::Transaction),
        "INVENTORY" => Ok(ScheduleReportType::Inventory),
        _ => Err(saved_invalid()),
    }
}
pub(crate) fn report_type_value(value: ScheduleReportType) -> &'static str {
    match value {
        ScheduleReportType::Transaction => "TRANSACTION",
        ScheduleReportType::Inventory => "INVENTORY",
    }
}
pub(crate) fn timestamp(value: NaiveDateTime) -> String {
    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc)
        .to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
pub(crate) fn saved_invalid() -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Saved report metadata is invalid",
    )
}
