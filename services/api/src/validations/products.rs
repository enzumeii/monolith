use crate::{
    error::AppError,
    models::shopify::{PageRequest, ShopifyId},
};
use axum::extract::{Query, rejection::QueryRejection};
use domain::dto::{ProductReportQuery, ProductReportType};

pub enum ReportSource {
    Inventory(ShopifyId),
    OrderTag(String),
}

pub struct ValidatedReport {
    pub source: ReportSource,
    pub page: PageRequest,
}
pub fn extract_report_query(
    query: Result<Query<ProductReportQuery>, QueryRejection>,
) -> Result<ValidatedReport, AppError> {
    let Query(query) = query.map_err(|_| {
        AppError::BadRequest(
            "Expected type INVENTORY or ORDER_TAG and an id with valid pagination".into(),
        )
        .at(module_path!())
    })?;
    validate_report_query(query)
}
pub fn validate_report_query(query: ProductReportQuery) -> Result<ValidatedReport, AppError> {
    let first = query.page_size.unwrap_or(10);
    if !(1..=50).contains(&first)
        || query
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.trim().is_empty())
    {
        return Err(AppError::BadRequest(
            "Expected page_size between 1 and 50 and a nonempty cursor".into(),
        )
        .at(module_path!()));
    }
    let page = PageRequest {
        first,
        after: query.cursor,
    };
    let invalid =
        || AppError::BadRequest("A valid report id is required".into()).at(module_path!());
    if query.id.trim().is_empty() {
        return Err(invalid());
    }
    let source = match query.report_type {
        ProductReportType::Inventory => {
            if !query.id.bytes().all(|c| c.is_ascii_digit()) || query.id.bytes().all(|c| c == b'0')
            {
                return Err(invalid());
            }
            let id = ShopifyId::new(format!("gid://shopify/Location/{}", query.id))
                .map_err(|_| invalid())?;
            ReportSource::Inventory(id)
        }
        ProductReportType::OrderTag => ReportSource::OrderTag(query.id),
    };
    Ok(ValidatedReport { source, page })
}
#[cfg(test)]
#[path = "../tests/validations/products.rs"]
mod tests;
