use crate::{
    models::{
        self,
        cron::report::{ReportResult, ResultFilter},
    },
    validations::cron::report::ValidatedFilter,
};
use domain::dto::{
    ReportFileDTO, ReportResultDTO, ScheduleReportType, report_results::ReportStatus,
};

pub(crate) fn filter(checked: &ValidatedFilter) -> ResultFilter {
    ResultFilter {
        report_type: checked.report_type,
        generated_from: checked.generated_from,
        generated_to: checked.generated_to,
        page: checked.page,
        per_page: checked.per_page,
    }
}

pub(crate) fn result(
    record: ReportResult,
    kind: ScheduleReportType,
    status: ReportStatus,
) -> ReportResultDTO {
    ReportResultDTO {
        id: record.id.to_string(),
        report_type: kind,
        attempted_at: models::cron::timestamp(record.attempted_at),
        generated_at: record.generated_at.map(models::cron::timestamp),
        status,
        error: record.error,
        parameters: record.parameters,
        files: record
            .files
            .into_iter()
            .map(|file| ReportFileDTO {
                filename: file.filename,
                content_type: file.content_type,
            })
            .collect(),
    }
}

pub(crate) fn download(
    download: crate::client::s3::PresignedDownload,
) -> domain::dto::ReportDownloadUrlDTO {
    domain::dto::ReportDownloadUrlDTO {
        url: download.url,
        expires_at: download
            .expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    }
}
