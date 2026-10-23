pub use super::common::PatchField;
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCronConfiguration {
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    pub parameter: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PatchCronConfiguration {
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub name: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub report_type: PatchField<ScheduleReportType>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub cron_expression: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub receiver_email: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub parameter: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub enabled: PatchField<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewCronConfiguration {
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub parameter: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CronPeriodPreview {
    pub period_start: String,
    pub period_end_exclusive: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CronOccurrencePreview {
    pub scheduled_for: String,
    pub period: Option<CronPeriodPreview>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CronConfigurationPreview {
    pub schedule_timezone: String,
    pub description: String,
    pub occurrences: Vec<CronOccurrencePreview>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CronConfigurationDTO {
    pub id: String,
    pub name: String,
    pub report_type: ScheduleReportType,
    pub cron_expression: String,
    pub receiver_email: String,
    pub parameter: String,
    pub enabled: bool,
    pub schedule_timezone: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListCronConfigurationsQuery {
    pub report_type: Option<ScheduleReportType>,
    pub enabled: Option<bool>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CronConfigurationListResponse {
    pub items: Vec<CronConfigurationDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
