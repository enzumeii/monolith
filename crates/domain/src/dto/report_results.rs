use super::cron::ScheduleReportType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListReportResultsQuery {
    pub report_type: Option<ScheduleReportType>,
    pub generated_from: Option<String>,
    pub generated_to: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportFileDTO {
    pub filename: String,
    pub content_type: String,
    pub download_url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportResultDTO {
    pub id: String,
    pub report_type: ScheduleReportType,
    pub attempted_at: String,
    pub generated_at: Option<String>,
    pub generation_status: ReportGenerationStatus,
    pub email_status: ReportEmailStatus,
    pub generation_error: Option<String>,
    pub email_error: Option<String>,
    pub period_start: Option<String>,
    pub period_end_exclusive: Option<String>,
    pub inventory_id: Option<String>,
    pub files: Vec<ReportFileDTO>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ReportGenerationStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportEmailStatus {
    NotReady,
    Pending,
    Sending,
    Sent,
    Failed,
    Unknown,
}
