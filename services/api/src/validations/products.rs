use crate::{error::AppError, models::shopify::ShopifyId};
use axum::extract::{Query, rejection::QueryRejection};
use domain::dto::{ProductReportQuery, ProductReportType};

pub enum ReportSource {
    Inventory(ShopifyId),
    OrderTag(String),
}

pub fn extract_report_query(
    query: Result<Query<ProductReportQuery>, QueryRejection>,
) -> Result<ReportSource, AppError> {
    let Query(query) = query.map_err(|_| {
        AppError::BadRequest("Expected type INVENTORY or ORDER_TAG and an id".into())
            .at(module_path!())
    })?;
    validate_report_query(query)
}
pub fn validate_report_query(query: ProductReportQuery) -> Result<ReportSource, AppError> {
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
    Ok(source)
}
#[cfg(test)]
#[path = "../tests/validations/products.rs"]
mod tests;
