use crate::{
    error::AppError,
    models::cron::ReportResult,
    repositories::{
        self,
        cron::{ReportPayload, ResultFilter},
    },
};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use domain::dto::{
    ListReportResultsQuery, ReportResultDTO, ReportResultDetail, ReportRowsResponse,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn list(pool: &PgPool, query: ListReportResultsQuery) -> Result<ReportPage, AppError> {
    let filter = make_filter(query)?;
    let page = filter.page;
    let per_page = filter.per_page;
    let mut records = repositories::cron::list_results(pool, filter).await?;
    let has_next = records.len() > per_page as usize;
    records.truncate(per_page as usize);
    Ok(ReportPage {
        items: records
            .into_iter()
            .map(|item| {
                let datasets = if item.generation_status == "SUCCEEDED" {
                    datasets(&item.report_type)
                } else {
                    Vec::new()
                };
                item.into_dto(datasets)
            })
            .collect(),
        page,
        per_page,
        has_next,
    })
}

pub async fn detail(pool: &PgPool, id: Uuid) -> Result<ReportResultDetail, AppError> {
    let record = repositories::cron::find_result(pool, id)
        .await?
        .ok_or_else(missing)?;
    let export_links = if record.generation_status == "SUCCEEDED" {
        datasets(&record.report_type)
            .into_iter()
            .map(|dataset| format!("/v1/admin/report-results/{id}/download?dataset={dataset}"))
            .collect()
    } else {
        Vec::new()
    };
    let available = export_links
        .iter()
        .filter_map(|link| link.rsplit('=').next().map(str::to_owned))
        .collect();
    Ok(ReportResultDetail {
        result: record.into_dto(available),
        export_links,
    })
}

pub async fn rows(
    pool: &PgPool,
    id: Uuid,
    dataset: &str,
    page: u32,
    per_page: u32,
) -> Result<ReportRowsResponse, AppError> {
    validate_page(page, per_page)?;
    let (record, payload) = successful_payload(pool, id).await?;
    validate_dataset(&payload, &record.report_type, dataset)?;
    let values = dataset_rows(payload.report_data.as_ref(), dataset)?;
    let columns = columns(&record.report_type, dataset, values.first());
    let start = ((page - 1) as usize).saturating_mul(per_page as usize);
    let end = start.saturating_add(per_page as usize);
    let has_next = values.len() > end;
    let items = values
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .collect();
    Ok(ReportRowsResponse {
        columns,
        items,
        page,
        per_page,
        has_next,
    })
}

pub async fn csv(pool: &PgPool, id: Uuid, dataset: &str) -> Result<(String, String), AppError> {
    let (record, payload) = successful_payload(pool, id).await?;
    validate_dataset(&payload, &record.report_type, dataset)?;
    let filename = safe_filename(&record.cron_name, dataset);
    let data = payload.report_data.as_ref().ok_or_else(missing)?;
    Ok((
        csv_from_saved(&record.report_type, data, dataset)?,
        filename,
    ))
}

pub fn csv_from_saved(report_type: &str, data: &Value, dataset: &str) -> Result<String, AppError> {
    if !datasets(report_type)
        .iter()
        .any(|candidate| candidate == dataset)
    {
        return Err(invalid("Dataset does not match report type"));
    }
    let values = dataset_rows(Some(data), dataset)?;
    let column_names = columns(report_type, dataset, values.first());
    let headers = column_names.iter().map(String::as_str).collect::<Vec<_>>();
    let rows = values
        .iter()
        .map(|row| {
            column_names
                .iter()
                .map(|name| {
                    let cell = row.get(name).map(cell_string).unwrap_or_default();
                    if row.get(name).is_some_and(Value::is_string) {
                        report_export::spreadsheet_safe_text(&cell)
                    } else {
                        cell
                    }
                })
                .collect()
        })
        .collect::<Vec<Vec<String>>>();
    Ok(report_export::encode_csv(&headers, &rows))
}

#[derive(serde::Serialize)]
pub struct ReportPage {
    pub items: Vec<ReportResultDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

async fn successful_payload(
    pool: &PgPool,
    id: Uuid,
) -> Result<(ReportResult, ReportPayload), AppError> {
    let record = repositories::cron::find_result(pool, id)
        .await?
        .ok_or_else(missing)?;
    let payload = repositories::cron::find_payload(pool, id)
        .await?
        .ok_or_else(missing)?;
    if payload.generation_status != "SUCCEEDED" || payload.report_data.is_none() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Report result is not ready",
        ));
    }
    if payload.report_schema_version != 1 {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Report schema version is unsupported",
        ));
    }
    if payload.report_type != record.report_type {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Saved report metadata is inconsistent",
        ));
    }
    Ok((record, payload))
}

fn make_filter(query: ListReportResultsQuery) -> Result<ResultFilter, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    validate_page(page, per_page)?;
    let configuration_id = query
        .configuration_id
        .map(|id| Uuid::parse_str(&id).map_err(|_| invalid("Invalid configuration_id")))
        .transpose()?;
    let scheduled_from = query
        .scheduled_from
        .as_deref()
        .map(parse_timestamp)
        .transpose()?;
    let scheduled_to = query
        .scheduled_to
        .as_deref()
        .map(parse_timestamp)
        .transpose()?;
    if scheduled_from
        .zip(scheduled_to)
        .is_some_and(|(from, to)| from >= to)
    {
        return Err(invalid("scheduled_from must be earlier than scheduled_to"));
    }
    Ok(ResultFilter {
        configuration_id,
        report_type: query.report_type,
        generation_status: query.generation_status,
        email_status: query.email_status,
        scheduled_from,
        scheduled_to,
        page,
        per_page,
    })
}

fn parse_timestamp(value: &str) -> Result<chrono::NaiveDateTime, AppError> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Timestamps must be RFC3339 with an explicit offset"))?;
    Ok(parsed.with_timezone(&Utc).naive_utc())
}

fn validate_page(page: u32, per_page: u32) -> Result<(), AppError> {
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(invalid(
            "page must be positive and per_page must be between 1 and 100",
        ));
    }
    Ok(())
}

fn datasets(report_type: &str) -> Vec<String> {
    match report_type {
        "TRANSACTION" => vec!["transactions".into(), "items".into()],
        "INVENTORY" => vec!["inventory".into()],
        _ => Vec::new(),
    }
}

fn validate_dataset(
    payload: &ReportPayload,
    report_type: &str,
    dataset: &str,
) -> Result<(), AppError> {
    if !datasets(report_type)
        .iter()
        .any(|candidate| candidate == dataset)
    {
        return Err(invalid("Dataset does not match report type"));
    }
    if dataset_rows(payload.report_data.as_ref(), dataset).is_err() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Report dataset is unavailable",
        ));
    }
    Ok(())
}

fn dataset_rows(data: Option<&Value>, dataset: &str) -> Result<Vec<Value>, AppError> {
    data.and_then(|value| value.get(dataset))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| AppError::new(StatusCode::CONFLICT, "Report dataset is unavailable"))
}

fn columns(report_type: &str, dataset: &str, first: Option<&Value>) -> Vec<String> {
    let fixed: &[&str] = match (report_type, dataset) {
        ("INVENTORY", "inventory") => &report_export::INVENTORY_HEADERS,
        ("TRANSACTION", "transactions") => &[
            "gateway",
            "type",
            "po_number",
            "po_date",
            "transaction_date",
            "payment_reference",
            "description",
            "shopify_order_id",
            "shopify_order_number",
            "customer_name",
            "customer_address",
            "customer_phone",
            "exchange_rate",
            "net_paid",
            "fee",
            "vat",
            "gross_paid",
            "shipping_fee",
            "order_adjusted",
        ],
        ("TRANSACTION", "items") => &[
            "quantity",
            "price",
            "currency",
            "title",
            "variant",
            "category",
            "sku",
            "kind",
            "transaction_date",
            "shopify_order_id",
            "payment_reference",
        ],
        _ => &[],
    };
    if !fixed.is_empty() {
        return fixed.iter().map(|name| (*name).to_owned()).collect();
    }
    first
        .and_then(Value::as_object)
        .map(|row| row.keys().cloned().collect())
        .unwrap_or_default()
}

fn cell_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => value.to_string(),
    }
}

fn safe_filename(name: &str, dataset: &str) -> String {
    let base = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let base = base.trim_matches('-');
    format!(
        "{}-{dataset}.csv",
        if base.is_empty() { "report" } else { base }
    )
}

fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Report result not found")
}
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../tests/services/report_results.rs"]
mod tests;
