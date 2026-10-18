use crate::{
    error::AppError,
    models::shopify::{PageRequest, ShopifyId},
};
use axum::extract::{
    Path, Query,
    rejection::{PathRejection, QueryRejection},
};
use axum::http::StatusCode;
use domain::dto::InventoryQuery;

pub fn extract_inventory_request(
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<InventoryQuery>, QueryRejection>,
) -> Result<(ShopifyId, PageRequest), AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a location id, page_size between 1 and 250, and a nonblank cursor",
        )
    };
    let Path(id) = path.map_err(|_| invalid())?;
    let Query(query) = query.map_err(|_| invalid())?;
    if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) || id.bytes().all(|c| c == b'0') {
        return Err(invalid());
    }
    let location = ShopifyId::new(format!("gid://shopify/Location/{id}")).map_err(|_| invalid())?;
    let page = PageRequest {
        first: query.page_size.unwrap_or(50),
        after: query.cursor,
    };
    page.validate().map_err(|_| invalid())?;
    Ok((location, page))
}

#[cfg(test)]
#[path = "../tests/validations/inventories.rs"]
mod tests;

pub fn extract_enable_request(
    path: Result<Path<String>, PathRejection>,
    query: Option<&str>,
) -> Result<ShopifyId, AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a positive location id and no query parameters",
        )
    };
    let Path(id) = path.map_err(|_| invalid())?;
    if query.is_some_and(|query| !query.is_empty())
        || id.is_empty()
        || !id.bytes().all(|byte| byte.is_ascii_digit())
        || id.bytes().all(|byte| byte == b'0')
    {
        return Err(invalid());
    }
    ShopifyId::new(format!("gid://shopify/Location/{id}")).map_err(|_| invalid())
}
