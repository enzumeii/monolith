use super::cron::{report_type, saved_invalid, timestamp};
use crate::error::AppError;
use chrono::NaiveDateTime;
use domain::dto::report_results::ReportStatus;
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
    pub status: String,
    pub error: Option<String>,
    pub parameters: String,
    pub files: Json<Vec<SavedFile>>,
}
impl ReportResult {
    pub fn into_dto(self) -> Result<ReportResultDTO, AppError> {
        let status = match self.status.as_str() {
            "PENDING" => ReportStatus::Pending,
            "SUCCESS" => ReportStatus::Success,
            "FAILED" => ReportStatus::Failed,
            _ => return Err(saved_invalid()),
        };
        let kind = report_type(&self.report_type)?;
        crate::validations::cron::saved_parameters(kind, &self.parameters)?;
        let mut files = Vec::new();
        for file in self.files.0 {
            crate::validations::report_results::saved_file(&file)?;
            files.push(ReportFileDTO {
                filename: file.filename,
                content_type: file.content_type,
                download_url: file.download_url,
            });
        }
        Ok(ReportResultDTO {
            id: self.id.to_string(),
            report_type: kind,
            attempted_at: timestamp(self.attempted_at),
            generated_at: self.generated_at.map(timestamp),
            status,
            error: self.error,
            parameters: self.parameters,
            files,
        })
    }
}
#[derive(Clone, Copy)]
pub(crate) struct ResultFilter {
    pub report_type: Option<ScheduleReportType>,
    pub generated_from: Option<NaiveDateTime>,
    pub generated_to: Option<NaiveDateTime>,
    pub page: u32,
    pub per_page: u32,
}
