use super::pagination::for_each_page;
use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::AppError,
    models::{
        products::ReportRow,
        shopify::{ListProductsRequest, OrdersByTagRequest, PageRequest, ProductStatus, ShopifyId},
    },
    state::AppState,
};
use domain::dto::{InventoryPageInfo, InventoryResponse, OrderReportResponse};
use std::collections::HashMap;

pub async fn inventory_report(
    state: &AppState,
    location_id: ShopifyId,
    page: PageRequest,
) -> Result<InventoryResponse, AppError> {
    let client = shopify_client(state)?;
    validate_location(client, &location_id).await?;
    let request = ListProductsRequest {
        location_id,
        statuses: vec![ProductStatus::Active],
    };
    let products = client
        .products_with_variants(&request, &page)
        .await
        .map_err(upstream)?;
    products
        .page_info
        .validate_after(page.after.as_deref())
        .map_err(upstream)?;
    let mut rows = Vec::new();
    for product in products.nodes {
        for variant in product.variants.into_complete_nodes().map_err(upstream)? {
            if let Some(row) = variant
                .into_report_row(&product.title, &product.product_type)
                .map_err(upstream)?
            {
                rows.push(row.into_dto());
            }
        }
    }
    Ok(InventoryResponse {
        products: rows,
        page_info: InventoryPageInfo {
            has_next_page: products.page_info.has_next_page,
            end_cursor: products.page_info.end_cursor,
        },
    })
}

pub async fn order_report(state: &AppState, tag: String) -> Result<OrderReportResponse, AppError> {
    let client = shopify_client(state)?;
    let request = OrdersByTagRequest { tag };
    let mut rows = Vec::new();
    let mut by_sku = HashMap::new();
    for_each_page(
        async |after| {
            client
                .orders_by_tag(&request, &PageRequest { first: 250, after })
                .await
        },
        |orders| {
            for order in orders {
                for line in order.line_items.into_complete_nodes()? {
                    merge(&mut rows, &mut by_sku, line.into_report_row()?)?;
                }
            }
            Ok(())
        },
    )
    .await
    .map_err(upstream)?;
    Ok(OrderReportResponse {
        products: rows.into_iter().map(ReportRow::into_dto).collect(),
    })
}
fn shopify_client(state: &AppState) -> Result<&ShopifyClient, AppError> {
    state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::ReportUnavailable(Some(error.clone())).at("api::client::shopify")
        })?
        .as_deref()
        .ok_or_else(|| AppError::ReportUnavailable(None).at(module_path!()))
}
async fn validate_location(client: &ShopifyClient, id: &ShopifyId) -> Result<(), AppError> {
    if !client.location_exists(id).await.map_err(upstream)? {
        return Err(AppError::NotFound.at(module_path!()));
    }
    Ok(())
}
fn upstream(error: ClientError) -> AppError {
    let origin = match &error {
        ClientError::Origin { origin, .. } => *origin,
        _ => "api::services::products",
    };
    AppError::ReportUpstream(error).at(origin)
}
fn merge(
    rows: &mut Vec<ReportRow>,
    by_sku: &mut HashMap<String, Vec<usize>>,
    row: ReportRow,
) -> Result<(), ClientError> {
    let candidates = row
        .sku()
        .map(|sku| by_sku.entry(sku.to_owned()).or_default());
    let existing = candidates
        .as_ref()
        .and_then(|indices| indices.iter().copied().find(|&i| rows[i].can_merge(&row)));
    if let Some(index) = existing {
        rows[index].add_quantity(row.quantity())
    } else {
        if let Some(indices) = candidates {
            indices.push(rows.len());
        }
        rows.push(row);
        Ok(())
    }
}
#[cfg(test)]
#[path = "../tests/services/products.rs"]
mod tests;
