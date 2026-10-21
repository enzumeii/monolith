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
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ReportStatus {
    Pending,
    Success,
    Failed,
}
#[derive(Clone, Debug, Serialize)]
pub struct ReportResultDTO {
    pub id: String,
    pub report_type: ScheduleReportType,
    pub attempted_at: String,
    pub generated_at: Option<String>,
    pub status: ReportStatus,
    pub error: Option<String>,
    pub parameters: String,
    pub files: Vec<ReportFileDTO>,
}
