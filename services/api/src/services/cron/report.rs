use crate::{
    client::{
        ClientError, paypal::PaypalClient, s3::S3Client, shopify::ShopifyClient,
        stripe::StripeClient,
    },
    error::AppError,
    models::{
        cron::{
            report::{ReportResult, ResultFilter},
            report_type,
        },
        shopify::{Connection, PageRequest, ShopifyId},
    },
    repositories, services,
};
use axum::http::StatusCode;
use chrono::NaiveDateTime;
use chrono_tz::Asia::Bangkok;
use domain::dto::{
    AccountingReportResponse, ProductReportRow, ReportResultDTO, ScheduleReportType,
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
            .map(ReportResult::into_dto)
            .collect::<Result<_, _>>()?,
        page: filter.page,
        per_page: filter.per_page,
        has_next,
    })
}
pub(crate) async fn detail(pool: &PgPool, id: Uuid) -> Result<ReportResultDTO, AppError> {
    repositories::report_results::find(pool, id)
        .await?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Report result not found"))?
        .into_dto()
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
    crate::validations::cron::report::validate_saved_file_for_report(
        file,
        report_type(&report.report_type)?,
        report.id,
        report.generated_at,
    )?;
    if file.bucket != client.bucket() {
        return Err(crate::models::cron::saved_invalid());
    }
    let object = client
        .metadata(&file.storage_key)
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
        .presign_download(&file.storage_key, &file.filename, ttl_secs)
        .await
        .map_err(|error| crate::error::upstream(error))?;
    Ok(domain::dto::ReportDownloadUrlDTO {
        url: download.url,
        expires_at: download
            .expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

pub(crate) async fn generate(
    pool: &PgPool,
    audit: &ReportResult,
    providers: Providers<'_>,
) -> Result<ReportData, AppError> {
    generate_report(pool, audit, providers).await
}

pub(crate) fn storage_prefix(
    kind: ScheduleReportType,
    generated_at: NaiveDateTime,
    id: Uuid,
) -> String {
    let day = generated_at
        .and_utc()
        .with_timezone(&Bangkok)
        .format("%Y%m%d");
    let report_type = match kind {
        ScheduleReportType::Inventory => "inventory",
        ScheduleReportType::Transaction => "transaction",
    };
    format!("{report_type}/{day}/{id}")
}

async fn generate_report(
    pool: &PgPool,
    audit: &ReportResult,
    providers: Providers<'_>,
) -> Result<ReportData, AppError> {
    match (report_type(&audit.report_type)?, providers) {
        (
            ScheduleReportType::Transaction,
            Providers::Transaction {
                shopify,
                stripe,
                paypal,
            },
        ) => {
            let range = crate::validations::cron::report::validate_saved_parameters(
                ScheduleReportType::Transaction,
                &audit.parameters,
            )?
            .ok_or_else(crate::models::cron::saved_invalid)?;
            Ok(ReportData::Transaction(
                services::accounting::report(pool, shopify, stripe, paypal, range).await?,
            ))
        }
        (ScheduleReportType::Inventory, Providers::Inventory(shopify)) => {
            let id = &audit.parameters;
            crate::validations::cron::report::validate_saved_parameters(
                ScheduleReportType::Inventory,
                id,
            )?;
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
