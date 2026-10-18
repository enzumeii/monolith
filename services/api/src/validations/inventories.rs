use crate::{
    error::AppError,
    models::shopify::{PageRequest, ShopifyId},
};
use axum::extract::{
    Path, Query,
    rejection::{PathRejection, QueryRejection},
};
use domain::dto::InventoryQuery;

pub fn extract_inventory_request(
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<InventoryQuery>, QueryRejection>,
) -> Result<(ShopifyId, PageRequest), AppError> {
    let invalid = || {
        AppError::BadRequest(
            "Expected a location id, page_size between 1 and 250, and a nonblank cursor".into(),
        )
        .at(module_path!())
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
