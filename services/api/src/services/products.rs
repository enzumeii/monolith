use super::pagination::for_each_page;
use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::AppError,
    models::{
        products::ReportRow,
        shopify::{ListProductsRequest, OrdersByTagRequest, PageRequest, ProductStatus, ShopifyId},
    },
    state::AppState,
    validations::products::ReportSource,
};
use domain::dto::ProductsResponse;
use std::collections::HashMap;

pub async fn product_report(
    state: &AppState,
    source: ReportSource,
) -> Result<ProductsResponse, AppError> {
    let client = shopify_client(state)?;
    let rows = match source {
        ReportSource::Inventory(location) => inventory_report(client, location).await?,
        ReportSource::OrderTag(tag) => tagged_report(client, tag).await?,
    };
    Ok(ProductsResponse {
        products: rows.into_iter().map(ReportRow::into_dto).collect(),
    })
}
async fn inventory_report(
    client: &ShopifyClient,
    location_id: ShopifyId,
) -> Result<Vec<ReportRow>, AppError> {
    validate_location(client, &location_id).await?;
    let request = ListProductsRequest {
        location_id,
        statuses: vec![ProductStatus::Active],
    };
    let mut rows = Vec::new();
    for_each_page(
        async |after| {
            client
                .products_with_variants(&request, &PageRequest { first: 1, after })
                .await
        },
        |products| {
            for product in products {
                for variant in product.variants.into_complete_nodes()? {
                    if let Some(row) =
                        variant.into_report_row(&product.title, &product.product_type)?
                    {
                        rows.push(row);
                    }
                }
            }
            Ok(())
        },
    )
    .await
    .map_err(upstream)?;
    Ok(rows)
}
async fn tagged_report(client: &ShopifyClient, tag: String) -> Result<Vec<ReportRow>, AppError> {
    let request = OrdersByTagRequest { tag };
    let mut rows = Vec::new();
    let mut by_sku = HashMap::new();
    for_each_page(
        async |after| {
            client
                .orders_by_tag(&request, &PageRequest { first: 1, after })
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
    Ok(rows)
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
