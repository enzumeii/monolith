use crate::{error::AppError, models::shopify::ShopifyId};
use domain::dto::{ProductReportQuery, ProductReportType};

pub enum ReportSource {
    Inventory(ShopifyId),
    OrderTag(String),
}

pub fn validate_report_query(query: ProductReportQuery) -> Result<ReportSource, AppError> {
    let invalid =
        || AppError::BadRequest("A valid report id is required".into()).at(module_path!());
    if query.id.trim().is_empty() {
        return Err(invalid());
    }
    match query.report_type {
        ProductReportType::Inventory => {
            if !query.id.bytes().all(|c| c.is_ascii_digit()) || query.id.bytes().all(|c| c == b'0')
            {
                return Err(invalid());
            }
            let id = ShopifyId::new(format!("gid://shopify/Location/{}", query.id))
                .map_err(|_| invalid())?;
            Ok(ReportSource::Inventory(id))
        }
        ProductReportType::OrderTag => Ok(ReportSource::OrderTag(query.id)),
    }
}
#[cfg(test)]
#[path = "../tests/validations/products.rs"]
mod tests;
