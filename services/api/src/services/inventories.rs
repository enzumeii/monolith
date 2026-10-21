mod enable;
pub use enable::enable_inventory;

use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::shopify::{ListProductsRequest, PageRequest, ProductStatus, ShopifyId},
};
use axum::http::StatusCode;
use domain::dto::InventoryResponse;

pub async fn inventory_report(
    client: &crate::client::shopify::ShopifyClient,
    location_id: ShopifyId,
    page: PageRequest,
) -> Result<InventoryResponse, AppError> {
    validate_location(client, &location_id).await?;
    let request = ListProductsRequest {
        location_id,
        statuses: vec![ProductStatus::Active],
    };
    let products = client
        .products_with_variants(&request, &page)
        .await
        .map_err(|error| upstream(error))?;
    products
        .validate_after(page.after.as_deref())
        .map_err(|error| upstream(error))?;
    let mut rows = Vec::new();
    for product in products.nodes {
        for variant in product
            .variants
            .into_complete_nodes()
            .map_err(|error| upstream(error))?
        {
            if let Some(row) = variant
                .into_report_row(&product.title, &product.product_type)
                .map_err(|error| upstream(error))?
            {
                rows.push(row.into_dto());
            }
        }
    }
    Ok(InventoryResponse {
        products: rows,
        page_info: products.page_info,
    })
}

async fn validate_location(client: &ShopifyClient, id: &ShopifyId) -> Result<(), AppError> {
    if !client
        .location_exists(id)
        .await
        .map_err(|error| upstream(error))?
    {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "Shopify location not found",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/services/inventories.rs"]
mod tests;
