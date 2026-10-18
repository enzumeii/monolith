use super::{
    common::{shopify_client, upstream},
    pagination::for_each_page,
};
use crate::{
    client::ClientError,
    error::AppError,
    models::{
        products::ReportRow,
        shopify::{OrdersByTagRequest, PageRequest},
    },
    state::AppState,
};
use domain::dto::OrderReportResponse;
use std::collections::HashMap;

pub async fn order_report(
    state: &AppState,
    tags: Vec<String>,
) -> Result<OrderReportResponse, AppError> {
    let client = shopify_client(state)?;
    let request = OrdersByTagRequest { tags };
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
        std::convert::identity,
    )
    .await
    .map_err(|error| upstream(error))?;
    Ok(OrderReportResponse {
        products: rows.into_iter().map(ReportRow::into_dto).collect(),
    })
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
#[path = "../tests/services/orders.rs"]
mod tests;

pub mod fulfillment;
mod shipping;
pub mod workflow;

pub mod reship;
