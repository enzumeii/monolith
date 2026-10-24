use super::inventories::parse_location_id;
use crate::{
    error::AppError,
    models::shopify::{PageRequest, ShopifyId},
};
use axum::{
    extract::{Query, rejection::QueryRejection},
    http::StatusCode,
};
use domain::dto::CatalogueQuery;

pub fn extract_catalogue_request(
    query: Result<Query<CatalogueQuery>, QueryRejection>,
) -> Result<(ShopifyId, PageRequest, Option<String>), AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a positive numeric location id, page_size between 1 and 250, and nonblank cursor/search up to 2048 bytes",
        )
    };
    let Query(query) = query.map_err(|_| invalid())?;
    let location = parse_location_id(&query.location_id).ok_or_else(invalid)?;
    location.number("Location").map_err(|_| invalid())?;
    for value in [&query.search, &query.cursor].into_iter().flatten() {
        if value.trim().is_empty() || value.len() > 2048 {
            return Err(invalid());
        }
    }
    let page = PageRequest {
        first: query.page_size.unwrap_or(25),
        after: query.cursor,
    };
    page.validate().map_err(|_| invalid())?;
    Ok((location, page, query.search))
}

#[cfg(test)]
#[path = "../tests/validations/catalogue.rs"]
mod tests;
