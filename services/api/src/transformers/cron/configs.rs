use crate::{
    error::AppError,
    models::{
        self,
        cron::{ConfigurationFilter, ConfigurationInput, CronConfiguration},
    },
};
use domain::dto::{
    CreateCronConfiguration, CronConfigurationDTO, CronConfigurationPreview, CronOccurrencePreview,
    CronPeriodPreview, ListCronConfigurationsQuery,
};

pub(crate) fn configuration(record: CronConfiguration) -> Result<CronConfigurationDTO, AppError> {
    Ok(CronConfigurationDTO {
        id: record.id.to_string(),
        name: record.name,
        report_type: models::cron::report_type(&record.report_type)?,
        cron_expression: record.cron_expression,
        receiver_email: record.receiver_email,
        parameter: record.parameter,
        enabled: record.enabled,
        schedule_timezone: "Asia/Bangkok".into(),
        created_at: models::cron::timestamp(record.created_at),
        updated_at: models::cron::timestamp(record.updated_at),
    })
}

pub(crate) fn input(request: CreateCronConfiguration, expression: String) -> ConfigurationInput {
    ConfigurationInput {
        name: request.name,
        receiver_email: request.receiver_email,
        cron_expression: expression,
        report_type: request.report_type,
        parameter: request.parameter,
        enabled: request.enabled,
    }
}

pub(crate) fn filter(
    query: &ListCronConfigurationsQuery,
    page: u32,
    per_page: u32,
) -> ConfigurationFilter {
    ConfigurationFilter {
        report_type: query.report_type,
        enabled: query.enabled,
        page,
        per_page,
    }
}

pub(crate) fn occurrence(
    scheduled_for: chrono::NaiveDateTime,
    period: Option<crate::services::cron::config::TransactionWindow>,
) -> CronOccurrencePreview {
    CronOccurrencePreview {
        scheduled_for: models::cron::timestamp(scheduled_for),
        period: period.map(|period| CronPeriodPreview {
            period_start: models::cron::timestamp(period.start),
            period_end_exclusive: models::cron::timestamp(period.end_exclusive),
        }),
    }
}

pub(crate) fn preview(
    description: String,
    occurrences: Vec<CronOccurrencePreview>,
) -> CronConfigurationPreview {
    CronConfigurationPreview {
        schedule_timezone: "Asia/Bangkok".into(),
        description,
        occurrences,
    }
}
