use super::pagination::collect_connection;
use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::AppError,
    models::{
        products::ReportRow,
        shopify::{
            ListProductsRequest, OrderLinesRequest, OrdersByTagRequest, PageInfo, PageRequest,
            ProductStatus, ProductVariantsRequest, ShopifyId,
        },
    },
    validations::products::{ReportSource, ValidatedReport},
};
use domain::dto::{ProductsResponse, products::ProductReportPageInfo};
use std::collections::HashMap;

pub async fn product_report(
    client: &ShopifyClient,
    input: ValidatedReport,
) -> Result<ProductsResponse, AppError> {
    let (rows, page_info) = match input.source {
        ReportSource::Inventory(location) => inventory_page(client, location, &input.page).await?,
        ReportSource::OrderTag(tag) => tagged_page(client, tag, &input.page).await?,
    };
    Ok(ProductsResponse {
        products: rows.into_iter().map(ReportRow::into_dto).collect(),
        page_info: ProductReportPageInfo {
            has_next_page: page_info.has_next_page,
            end_cursor: page_info.end_cursor,
        },
    })
}
async fn inventory_page(
    client: &ShopifyClient,
    location_id: ShopifyId,
    page: &PageRequest,
) -> Result<(Vec<ReportRow>, PageInfo), AppError> {
    validate_location(client, &location_id).await?;
    let catalog = client
        .products_with_variants(
            &ListProductsRequest {
                location_id: location_id.clone(),
                statuses: vec![ProductStatus::Active],
                variants_first: 10,
            },
            page,
        )
        .await
        .map_err(upstream)?;
    catalog
        .page_info
        .validate_after(page.after.as_deref())
        .map_err(upstream)?;
    let mut rows = Vec::new();
    for product in catalog.nodes {
        let id = product.id;
        let mut initial = Some(product.variants);
        let variants = collect_connection(|after| {
            let first = initial.take();
            let location_id = &location_id;
            let id = &id;
            async move {
                if let Some(first) = first {
                    return Ok(first);
                }
                client
                    .product_variants(
                        &ProductVariantsRequest {
                            product_id: id.clone(),
                            location_id: location_id.clone(),
                        },
                        &PageRequest { first: 100, after },
                    )
                    .await?
                    .into_connection(id)
            }
        })
        .await
        .map_err(upstream)?;
        for variant in variants.nodes {
            if let Some(row) = variant
                .into_report_row(&product.title, &product.product_type)
                .map_err(upstream)?
            {
                rows.push(row);
            }
        }
    }
    Ok((rows, catalog.page_info))
}
async fn tagged_page(
    client: &ShopifyClient,
    tag: String,
    page: &PageRequest,
) -> Result<(Vec<ReportRow>, PageInfo), AppError> {
    let orders = client
        .orders_by_tag(
            &OrdersByTagRequest {
                tag,
                lines_first: 10,
            },
            page,
        )
        .await
        .map_err(upstream)?;
    orders
        .page_info
        .validate_after(page.after.as_deref())
        .map_err(upstream)?;
    let mut rows = Vec::new();
    let mut by_sku = HashMap::new();
    for order in orders.nodes {
        let id = order.id;
        let mut initial = Some(order.line_items);
        let lines = collect_connection(|after| {
            let first = initial.take();
            let id = &id;
            async move {
                if let Some(first) = first {
                    return Ok(first);
                }
                client
                    .tagged_order_lines(
                        &OrderLinesRequest {
                            order_id: id.clone(),
                        },
                        &PageRequest { first: 100, after },
                    )
                    .await?
                    .into_connection(id)
            }
        })
        .await
        .map_err(upstream)?;
        for line in lines.nodes {
            merge(
                &mut rows,
                &mut by_sku,
                line.into_report_row().map_err(upstream)?,
            )?;
        }
    }
    Ok((rows, orders.page_info))
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
) -> Result<(), AppError> {
    let candidates = row
        .sku()
        .map(|sku| by_sku.entry(sku.to_owned()).or_default());
    let existing = candidates
        .as_ref()
        .and_then(|indices| indices.iter().copied().find(|&i| rows[i].can_merge(&row)));
    if let Some(index) = existing {
        rows[index].add_quantity(row.quantity()).map_err(upstream)
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
