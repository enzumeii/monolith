use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ScheduleReportType {
    Transaction,
    Inventory,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchedulePeriodMode {
    PreviousMonth,
    FirstHalfMonth,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ScheduleStatus {
    Enabled,
    #[default]
    Disabled,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GenerationStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EmailStatus {
    NotReady,
    Pending,
    Sending,
    Sent,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCronConfiguration {
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    #[serde(default)]
    pub period_mode: Option<SchedulePeriodMode>,
    #[serde(default)]
    pub inventory_id: Option<String>,
    #[serde(default)]
    pub status: ScheduleStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PatchField<T> {
    #[default]
    Missing,
    Clear,
    Set(T),
}

impl<'de, T> Deserialize<'de> for PatchField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Set(value),
            None => Self::Clear,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PatchCronConfiguration {
    pub name: PatchField<String>,
    pub report_type: PatchField<ScheduleReportType>,
    pub cron_expression: PatchField<String>,
    pub receiver_email: PatchField<String>,
    pub period_mode: PatchField<SchedulePeriodMode>,
    pub inventory_id: PatchField<String>,
    pub status: PatchField<ScheduleStatus>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewCronConfiguration {
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    #[serde(default)]
    pub period_mode: Option<SchedulePeriodMode>,
    #[serde(default)]
    pub inventory_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CronPeriodPreview {
    pub period_start: String,
    pub period_end_exclusive: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CronOccurrencePreview {
    pub scheduled_for: String,
    pub period: Option<CronPeriodPreview>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CronConfigurationPreview {
    pub schedule_timezone: String,
    pub description: String,
    pub occurrences: Vec<CronOccurrencePreview>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CronConfigurationDTO {
    pub id: String,
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    pub period_mode: Option<SchedulePeriodMode>,
    pub inventory_id: Option<String>,
    pub status: ScheduleStatus,
    pub schedule_timezone: &'static str,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListCronConfigurationsQuery {
    #[serde(default)]
    pub report_type: Option<ScheduleReportType>,
    #[serde(default)]
    pub status: Option<ScheduleStatus>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CronConfigurationListResponse {
    pub items: Vec<CronConfigurationDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListReportResultsQuery {
    #[serde(default)]
    pub configuration_id: Option<String>,
    #[serde(default)]
    pub report_type: Option<ScheduleReportType>,
    #[serde(default)]
    pub generation_status: Option<GenerationStatus>,
    #[serde(default)]
    pub email_status: Option<EmailStatus>,
    #[serde(default)]
    pub scheduled_from: Option<String>,
    #[serde(default)]
    pub scheduled_to: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportResultDTO {
    pub id: String,
    pub configuration_id_snapshot: String,
    pub cron_name: String,
    pub cron_expression: String,
    pub report_type: ScheduleReportType,
    pub receiver_email: String,
    pub period_mode: Option<SchedulePeriodMode>,
    pub inventory_id: Option<String>,
    pub scheduled_for: String,
    pub period_start: Option<String>,
    pub period_end_exclusive: Option<String>,
    pub generation_status: GenerationStatus,
    pub email_status: EmailStatus,
    pub report_schema_version: i32,
    pub row_count: Option<i64>,
    pub started_at: Option<String>,
    pub generated_at: Option<String>,
    pub email_first_attempt_at: Option<String>,
    pub sent_at: Option<String>,
    pub generation_error: Option<String>,
    pub email_error: Option<String>,
    pub email_message_id: Option<String>,
    pub available_datasets: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportResultDetail {
    #[serde(flatten)]
    pub result: ReportResultDTO,
    pub export_links: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRowsQuery {
    pub dataset: String,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReportRowsResponse {
    pub columns: Vec<String>,
    pub items: Vec<serde_json::Value>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
