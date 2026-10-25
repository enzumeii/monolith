use chrono::NaiveDateTime;
use domain::dto::ScheduleReportType;
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

pub(crate) fn valid_inventory_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('0') && id.bytes().all(|byte| byte.is_ascii_digit())
}
