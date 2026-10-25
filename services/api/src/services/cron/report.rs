use crate::{
    client::{
        ClientError, paypal::PaypalClient, s3::S3Client, shopify::ShopifyClient,
        stripe::StripeClient,
    },
    error::AppError,
    models::{
        self,
        cron::report::{ReportResult, ResultFilter},
        shopify::{Connection, PageRequest, ShopifyId},
    },
    repositories, services, transformers,
};
use axum::http::StatusCode;
use chrono::Timelike;
use domain::dto::{
    AccountingReportResponse, ProductReportRow, ReportResultDTO, ScheduleReportType,
    report_results::ReportStatus,
};
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) enum Providers<'a> {
    Inventory(&'a ShopifyClient),
    Transaction {
        shopify: &'a ShopifyClient,
        stripe: &'a StripeClient,
        paypal: &'a PaypalClient,
    },
}
pub(crate) enum ReportData {
    Inventory(Vec<ProductReportRow>),
    Transaction(AccountingReportResponse),
}
#[derive(serde::Serialize)]
pub struct ReportPage {
    pub items: Vec<ReportResultDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
pub(crate) async fn list(pool: &PgPool, filter: ResultFilter) -> Result<ReportPage, AppError> {
    let mut records = repositories::report_results::list(pool, filter).await?;
    let has_next = records.len() > filter.per_page as usize;
    records.truncate(filter.per_page as usize);
    Ok(ReportPage {
        items: records
            .into_iter()
            .map(result_dto)
            .collect::<Result<_, _>>()?,
        page: filter.page,
        per_page: filter.per_page,
        has_next,
    })
}
pub(crate) async fn detail(pool: &PgPool, id: Uuid) -> Result<ReportResultDTO, AppError> {
    result_dto(
        repositories::report_results::find(pool, id)
            .await?
            .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Report result not found"))?,
    )
}
pub(crate) async fn download_url(
    pool: &PgPool,
    client: &S3Client,
    id: Uuid,
    filename: &str,
    ttl_secs: u32,
) -> Result<domain::dto::ReportDownloadUrlDTO, AppError> {
    let report = repositories::report_results::find(pool, id)
        .await?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Report result not found"))?;
    let file = report
        .files
        .iter()
        .find(|file| file.filename == filename)
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Report file not found"))?;
    file.validate_for_report(
        models::cron::report_type(&report.report_type)?,
        report.id,
        report.generated_at,
    )?;
    if file.bucket != client.bucket() {
        return Err(crate::models::cron::saved_invalid());
    }
    let object = client
        .metadata(client.bucket(), &file.storage_key)
        .await
        .map_err(|error| crate::error::upstream(error))?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Report file not found"))?;
    if object.storage_key != file.storage_key
        || object.size != file.size_bytes as u64
        || object.content_type != file.content_type
    {
        return Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            "Upstream request failed",
        ));
    }
    let download = client
        .presign_download(client.bucket(), &file.storage_key, &file.filename, ttl_secs)
        .await
        .map_err(|error| crate::error::upstream(error))?;
    Ok(transformers::cron::report::download(download))
}

pub(crate) fn result_dto(record: ReportResult) -> Result<ReportResultDTO, AppError> {
    let status = match record.status.as_str() {
        "PENDING" => ReportStatus::Pending,
        "SUCCESS" => ReportStatus::Success,
        "FAILED" => ReportStatus::Failed,
        _ => return Err(crate::models::cron::saved_invalid()),
    };
    let kind = models::cron::report_type(&record.report_type)?;
    saved_parameters(kind, &record.parameters)?;
    for file in &record.files {
        file.validate_for_report(kind, record.id, record.generated_at)?;
    }
    Ok(transformers::cron::report::result(record, kind, status))
}

pub(crate) fn resolve_parameters(
    kind: ScheduleReportType,
    value: &str,
    due: chrono::NaiveDateTime,
) -> Result<String, AppError> {
    if kind == ScheduleReportType::Inventory {
        if !crate::models::cron::configs::valid_inventory_id(value) {
            return Err(invalid(
                "Inventory schedules require a positive inventory ID",
            ));
        }
        return Ok(value.into());
    }
    let window = crate::services::cron::config::transaction_window(
        services::cron::config::validate_period(value)?,
        due.and_utc(),
    )
    .map_err(|_| invalid("Invalid report period"))?;
    let start = window
        .start
        .and_utc()
        .with_timezone(&chrono_tz::Asia::Bangkok);
    let end = (window.end_exclusive - chrono::Duration::seconds(1))
        .and_utc()
        .with_timezone(&chrono_tz::Asia::Bangkok);
    Ok(format!(
        "{} - {}",
        start.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        end.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
    ))
}
pub(crate) fn saved_parameters(
    kind: ScheduleReportType,
    value: &str,
) -> Result<Option<crate::models::accounting::AccountingDateRange>, AppError> {
    if kind == ScheduleReportType::Inventory {
        return if crate::models::cron::configs::valid_inventory_id(value) {
            Ok(None)
        } else {
            Err(crate::models::cron::saved_invalid())
        };
    }
    let invalid = crate::models::cron::saved_invalid;
    let (start, end) = value.split_once(" - ").ok_or_else(invalid)?;
    let start = chrono::DateTime::parse_from_rfc3339(start).map_err(|_| invalid())?;
    let end = chrono::DateTime::parse_from_rfc3339(end).map_err(|_| invalid())?;
    if start >= end
        || start.offset().local_minus_utc() != 25200
        || end.offset().local_minus_utc() != 25200
        || (
            start.hour(),
            start.minute(),
            start.second(),
            start.nanosecond(),
        ) != (0, 0, 0, 0)
        || (end.hour(), end.minute(), end.second(), end.nanosecond()) != (23, 59, 59, 0)
    {
        return Err(invalid());
    }
    Ok(Some(crate::models::accounting::AccountingDateRange {
        start,
        end: end + chrono::Duration::seconds(1) - chrono::Duration::nanoseconds(1),
    }))
}

pub(crate) async fn generate(
    audit: &ReportResult,
    providers: Providers<'_>,
) -> Result<ReportData, AppError> {
    match (models::cron::report_type(&audit.report_type)?, providers) {
        (
            ScheduleReportType::Transaction,
            Providers::Transaction {
                shopify,
                stripe,
                paypal,
            },
        ) => {
            let range = saved_parameters(ScheduleReportType::Transaction, &audit.parameters)?
                .ok_or_else(crate::models::cron::saved_invalid)?;
            Ok(ReportData::Transaction(
                services::accounting::report(shopify, stripe, paypal, range).await?,
            ))
        }
        (ScheduleReportType::Inventory, Providers::Inventory(shopify)) => {
            let id = &audit.parameters;
            saved_parameters(ScheduleReportType::Inventory, id)?;
            let location =
                ShopifyId::new(format!("gid://shopify/Location/{id}")).map_err(|error| {
                    AppError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Saved inventory location is invalid",
                    )
                    .with_source(error)
                })?;
            let mut products = Vec::new();
            services::pagination::for_each_page(
                None,
                async |after| {
                    let page = services::inventories::inventory_report(
                        shopify,
                        location.clone(),
                        PageRequest { first: 250, after },
                    )
                    .await?;
                    if page.page_info.has_next_page && page.page_info.end_cursor.is_none() {
                        return Err(AppError::new(
                            StatusCode::BAD_GATEWAY,
                            "Shopify inventory pagination cursor is missing",
                        ));
                    }
                    Ok(Connection {
                        nodes: page.products,
                        page_info: page.page_info,
                    })
                },
                |rows| {
                    products.extend(rows);
                    Ok(())
                },
                |error: ClientError| {
                    AppError::new(
                        StatusCode::BAD_GATEWAY,
                        "Shopify inventory pagination cursor did not advance",
                    )
                    .with_source(error)
                },
            )
            .await?;
            Ok(ReportData::Inventory(products))
        }
        _ => Err(crate::models::cron::saved_invalid()),
    }
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../../tests/services/cron/report.rs"]
mod tests;
