use crate::{
    error::AppError,
    models::{
        cron::report_type,
        report_results::{ReportResult, ResultFilter},
        shopify::{PageRequest, ShopifyId},
    },
    repositories, services,
    state::AppState,
};
use axum::http::StatusCode;
use domain::dto::{
    AccountingReportResponse, ProductReportRow, ReportResultDTO, ScheduleReportType,
};
use services::report_storage::{self, CSV_CONTENT_TYPE, GeneratedFile, PreparedReport};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

enum ReportData {
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
pub(crate) async fn generate(
    state: &AppState,
    audit: &ReportResult,
) -> Result<PreparedReport, AppError> {
    let data = generate_report(state, audit).await?;
    let generated_at = chrono::Utc::now().naive_utc();
    let files = tokio::task::spawn_blocking(move || encode(data))
        .await
        .map_err(anyhow::Error::new)?;
    if files.iter().map(|file| file.bytes.len()).sum::<usize>()
        > state.settings.scheduler.max_payload_bytes
    {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Report files exceed configured size limit",
        ));
    }
    report_storage::prepare(
        state.settings.reports.storage.directory(),
        audit.id,
        generated_at,
        files,
    )
    .await
}
fn encode(data: ReportData) -> Vec<GeneratedFile> {
    let pairs = match data {
        ReportData::Inventory(rows) => {
            vec![("inventory.csv", report_generator::csv::encode(&rows))]
        }
        ReportData::Transaction(report) => {
            let transactions = report_generator::accounting::transaction_table(
                report
                    .transactions
                    .stripe
                    .iter()
                    .chain(&report.transactions.paypal),
                "",
            );
            let items = report_generator::accounting::item_table(&report.items);
            vec![
                (
                    "transactions.csv",
                    report_generator::csv::encode_table(transactions.headers, &transactions.rows),
                ),
                (
                    "items.csv",
                    report_generator::csv::encode_table(items.headers, &items.rows),
                ),
            ]
        }
    };
    pairs
        .into_iter()
        .map(|(filename, csv)| GeneratedFile {
            filename: filename.into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: csv.into_bytes(),
        })
        .collect()
}
async fn generate_report(state: &AppState, job: &ReportResult) -> Result<ReportData, AppError> {
    match report_type(&job.report_type)? {
        ScheduleReportType::Transaction => {
            let range = crate::validations::cron::saved_parameters(
                ScheduleReportType::Transaction,
                &job.parameters,
            )?
            .ok_or_else(crate::models::cron::saved_invalid)?;
            let result: AccountingReportResponse =
                services::accounting::report(state, range).await?;
            Ok(ReportData::Transaction(result))
        }
        ScheduleReportType::Inventory => {
            let id = &job.parameters;
            crate::validations::cron::saved_parameters(ScheduleReportType::Inventory, id)?;
            let location =
                ShopifyId::new(format!("gid://shopify/Location/{id}")).map_err(|error| {
                    AppError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Saved inventory location is invalid",
                    )
                    .with_source(error)
                })?;
            let mut cursor = None;
            let mut seen_cursors = HashSet::new();
            let mut products = Vec::new();
            loop {
                let page = services::inventories::inventory_report(
                    state,
                    location.clone(),
                    PageRequest {
                        first: 250,
                        after: cursor.clone(),
                    },
                )
                .await?;
                products.extend(page.products);
                if !page.page_info.has_next_page {
                    break;
                }
                let next = page.page_info.end_cursor.ok_or_else(|| {
                    AppError::new(
                        StatusCode::BAD_GATEWAY,
                        "Shopify inventory pagination cursor is missing",
                    )
                })?;
                if !seen_cursors.insert(next.clone()) {
                    return Err(AppError::new(
                        StatusCode::BAD_GATEWAY,
                        "Shopify inventory pagination cursor did not advance",
                    ));
                }
                cursor = Some(next);
            }
            Ok(ReportData::Inventory(products))
        }
    }
}
