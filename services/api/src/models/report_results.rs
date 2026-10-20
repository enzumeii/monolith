use super::cron::{report_type, timestamp};
use chrono::NaiveDateTime;
use domain::dto::{ReportFileDTO, ReportResultDTO, ScheduleReportType};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SavedFile {
    pub filename: String,
    pub content_type: String,
    pub download_url: String,
    pub storage_key: String,
}

#[derive(FromRow)]
pub(crate) struct ReportResult {
    pub id: Uuid,
    pub report_type: String,
    pub attempted_at: NaiveDateTime,
    pub generated_at: Option<NaiveDateTime>,
    pub generation_status: String,
    pub email_status: String,
    pub generation_error: Option<String>,
    pub email_error: Option<String>,
    pub period_start: Option<NaiveDateTime>,
    pub period_end_exclusive: Option<NaiveDateTime>,
    pub inventory_id: Option<String>,
    pub files: Json<Vec<SavedFile>>,
}

impl ReportResult {
    pub fn into_dto(self) -> ReportResultDTO {
        ReportResultDTO {
            id: self.id.to_string(),
            report_type: report_type(&self.report_type),
            attempted_at: timestamp(self.attempted_at),
            generated_at: self.generated_at.map(timestamp),
            generation_status: match self.generation_status.as_str() {
                "RUNNING" => domain::dto::report_results::ReportGenerationStatus::Running,
                "SUCCEEDED" => domain::dto::report_results::ReportGenerationStatus::Succeeded,
                "FAILED" => domain::dto::report_results::ReportGenerationStatus::Failed,
                _ => unreachable!("database restricts generation status"),
            },
            email_status: match self.email_status.as_str() {
                "NOT_READY" => domain::dto::report_results::ReportEmailStatus::NotReady,
                "PENDING" => domain::dto::report_results::ReportEmailStatus::Pending,
                "SENDING" => domain::dto::report_results::ReportEmailStatus::Sending,
                "SENT" => domain::dto::report_results::ReportEmailStatus::Sent,
                "FAILED" => domain::dto::report_results::ReportEmailStatus::Failed,
                "UNKNOWN" => domain::dto::report_results::ReportEmailStatus::Unknown,
                _ => unreachable!("database restricts email status"),
            },
            generation_error: self.generation_error,
            email_error: self.email_error,
            period_start: self.period_start.map(timestamp),
            period_end_exclusive: self.period_end_exclusive.map(timestamp),
            inventory_id: self.inventory_id,
            files: self
                .files
                .0
                .into_iter()
                .map(|file| ReportFileDTO {
                    filename: file.filename,
                    content_type: file.content_type,
                    download_url: file.download_url,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResultFilter {
    pub report_type: Option<ScheduleReportType>,
    pub generated_from: Option<NaiveDateTime>,
    pub generated_to: Option<NaiveDateTime>,
    pub page: u32,
    pub per_page: u32,
}
