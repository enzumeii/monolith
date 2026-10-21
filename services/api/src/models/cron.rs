use crate::error::AppError;
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use domain::dto::{CronConfigurationDTO, ScheduleReportType};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Clone, FromRow)]
pub(crate) struct CronConfiguration {
    pub id: Uuid,
    pub name: String,
    pub report_type: String,
    pub cron_expression: String,
    pub receiver_email: String,
    pub parameter: String,
    pub enabled: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
impl CronConfiguration {
    pub fn into_dto(self) -> Result<CronConfigurationDTO, AppError> {
        Ok(CronConfigurationDTO {
            id: self.id.to_string(),
            name: self.name,
            report_type: report_type(&self.report_type)?,
            cron_expression: self.cron_expression,
            receiver_email: self.receiver_email,
            parameter: self.parameter,
            enabled: self.enabled,
            schedule_timezone: "Asia/Bangkok",
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        })
    }
}
#[derive(Clone, Copy)]
pub(crate) struct ConfigurationFilter {
    pub report_type: Option<ScheduleReportType>,
    pub enabled: Option<bool>,
    pub page: u32,
    pub per_page: u32,
}
pub(crate) struct ConfigurationInput {
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    pub parameter: String,
    pub enabled: bool,
}
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
