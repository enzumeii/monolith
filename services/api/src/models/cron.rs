use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use domain::dto::{
    CronConfigurationDTO, ReportFileFormat, SchedulePeriodMode, ScheduleReportType, ScheduleStatus,
};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct CronConfiguration {
    pub id: Uuid,
    pub name: String,
    pub report_type: String,
    pub cron_expression: String,
    pub receiver_email: String,
    pub file_format: String,
    pub period_mode: Option<String>,
    pub inventory_id: Option<String>,
    pub status: String,
    pub next_run_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl CronConfiguration {
    pub fn into_dto(self) -> CronConfigurationDTO {
        CronConfigurationDTO {
            id: self.id.to_string(),
            name: self.name,
            report_type: report_type(&self.report_type),
            cron_expression: self.cron_expression,
            receiver_email: self.receiver_email,
            file_format: file_format(&self.file_format),
            period_mode: self.period_mode.as_deref().map(period_mode),
            inventory_id: self.inventory_id,
            status: status(&self.status),
            schedule_timezone: "Asia/Bangkok",
            next_run_at: self.next_run_at.map(timestamp),
            created_at: timestamp(self.created_at),
            updated_at: timestamp(self.updated_at),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ConfigurationFilter {
    pub report_type: Option<ScheduleReportType>,
    pub status: Option<ScheduleStatus>,
    pub page: u32,
    pub per_page: u32,
}

#[derive(FromRow)]
pub(crate) struct Job {
    pub id: Uuid,
    pub receiver_email: String,
    pub report_type: String,
    pub file_format: String,
    pub period_start: Option<NaiveDateTime>,
    pub period_end_exclusive: Option<NaiveDateTime>,
    pub inventory_id: Option<String>,
    pub generation_attempts: i32,
    pub email_attempts: i32,
    pub publication_attempts: i32,
}

pub(crate) fn file_format(value: &str) -> ReportFileFormat {
    match value {
        "CSV" => ReportFileFormat::Csv,
        "XLSX" => ReportFileFormat::Xlsx,
        _ => unreachable!("database check restricts file formats"),
    }
}

pub(crate) fn report_type(value: &str) -> ScheduleReportType {
    match value {
        "TRANSACTION" => ScheduleReportType::Transaction,
        "INVENTORY" => ScheduleReportType::Inventory,
        _ => unreachable!("database check restricts report types"),
    }
}

pub(crate) fn period_mode(value: &str) -> SchedulePeriodMode {
    match value {
        "PREVIOUS_MONTH" => SchedulePeriodMode::PreviousMonth,
        "FIRST_HALF_MONTH" => SchedulePeriodMode::FirstHalfMonth,
        _ => unreachable!("database check restricts period modes"),
    }
}

pub(crate) fn status(value: &str) -> ScheduleStatus {
    match value {
        "ENABLED" => ScheduleStatus::Enabled,
        "DISABLED" => ScheduleStatus::Disabled,
        _ => unreachable!("database check restricts schedule statuses"),
    }
}

pub(crate) fn timestamp(value: NaiveDateTime) -> String {
    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc)
        .to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
