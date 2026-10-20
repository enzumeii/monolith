use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use domain::dto::{
    CronConfigurationDTO, EmailStatus, GenerationStatus, ReportResultDTO, SchedulePeriodMode,
    ScheduleReportType, ScheduleStatus,
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

#[derive(FromRow)]
pub(crate) struct ReportResult {
    pub id: Uuid,
    pub configuration_id_snapshot: Uuid,
    pub cron_name: String,
    pub cron_expression: String,
    pub report_type: String,
    pub receiver_email: String,
    pub period_mode: Option<String>,
    pub inventory_id: Option<String>,
    pub scheduled_for: NaiveDateTime,
    pub period_start: Option<NaiveDateTime>,
    pub period_end_exclusive: Option<NaiveDateTime>,
    pub generation_status: String,
    pub email_status: String,
    pub report_schema_version: i32,
    pub row_count: Option<i64>,
    pub started_at: Option<NaiveDateTime>,
    pub generated_at: Option<NaiveDateTime>,
    pub email_first_attempt_at: Option<NaiveDateTime>,
    pub sent_at: Option<NaiveDateTime>,
    pub generation_error: Option<String>,
    pub email_error: Option<String>,
    pub email_message_id: Option<String>,
}

impl ReportResult {
    pub fn into_dto(self, datasets: Vec<String>) -> ReportResultDTO {
        ReportResultDTO {
            id: self.id.to_string(),
            configuration_id_snapshot: self.configuration_id_snapshot.to_string(),
            cron_name: self.cron_name,
            cron_expression: self.cron_expression,
            report_type: report_type(&self.report_type),
            receiver_email: self.receiver_email,
            period_mode: self.period_mode.as_deref().map(period_mode),
            inventory_id: self.inventory_id,
            scheduled_for: timestamp(self.scheduled_for),
            period_start: self.period_start.map(timestamp),
            period_end_exclusive: self.period_end_exclusive.map(timestamp),
            generation_status: generation_status(&self.generation_status),
            email_status: email_status(&self.email_status),
            report_schema_version: self.report_schema_version,
            row_count: self.row_count,
            started_at: self.started_at.map(timestamp),
            generated_at: self.generated_at.map(timestamp),
            email_first_attempt_at: self.email_first_attempt_at.map(timestamp),
            sent_at: self.sent_at.map(timestamp),
            generation_error: self.generation_error,
            email_error: self.email_error,
            email_message_id: self.email_message_id,
            available_datasets: datasets,
        }
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

fn generation_status(value: &str) -> GenerationStatus {
    match value {
        "QUEUED" => GenerationStatus::Queued,
        "RUNNING" => GenerationStatus::Running,
        "SUCCEEDED" => GenerationStatus::Succeeded,
        "FAILED" => GenerationStatus::Failed,
        _ => unreachable!("database check restricts generation statuses"),
    }
}

fn email_status(value: &str) -> EmailStatus {
    match value {
        "NOT_READY" => EmailStatus::NotReady,
        "PENDING" => EmailStatus::Pending,
        "SENDING" => EmailStatus::Sending,
        "SENT" => EmailStatus::Sent,
        "FAILED" => EmailStatus::Failed,
        "UNKNOWN" => EmailStatus::Unknown,
        _ => unreachable!("database check restricts email statuses"),
    }
}

fn timestamp(value: NaiveDateTime) -> String {
    DateTime::<Utc>::from_naive_utc_and_offset(value, Utc)
        .to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
