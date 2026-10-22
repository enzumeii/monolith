use crate::{
    error::AppError,
    models::cron::{report_type, timestamp},
};
use chrono::NaiveDateTime;
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
