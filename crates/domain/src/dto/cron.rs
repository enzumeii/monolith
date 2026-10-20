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

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ReportFileFormat {
    #[default]
    Csv,
    Xlsx,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCronConfiguration {
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    #[serde(default)]
    pub file_format: ReportFileFormat,
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
    pub file_format: PatchField<ReportFileFormat>,
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
    pub file_format: ReportFileFormat,
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
