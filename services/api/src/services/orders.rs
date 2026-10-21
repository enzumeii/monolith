use super::pagination::for_each_page;
use crate::{
    client::ClientError,
    error::{AppError, upstream},
    models::{
        products::ReportRow,
        shopify::{OrdersByTagRequest, PageRequest},
    },
};
use domain::dto::{
    OrderReportResponse,
    orders::{ListOrdersResponse, OrderSummary},
};
use std::collections::HashMap;

pub async fn order_report(
    client: &crate::client::shopify::ShopifyClient,
    tags: Vec<String>,
) -> Result<OrderReportResponse, AppError> {
    let request = OrdersByTagRequest { tags };
    let mut rows = Vec::new();
    let mut by_sku = HashMap::new();
    for_each_page(
        None,
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

pub async fn list(
    client: &crate::client::shopify::ShopifyClient,
    search: String,
    page: PageRequest,
) -> Result<ListOrdersResponse, AppError> {
    let result = client
        .list_orders(&search, &page)
        .await
        .map_err(|error| upstream(error))?;
    result
        .validate_after(page.after.as_deref())
        .map_err(|error| upstream(error))?;
    let orders = result
        .nodes
        .into_iter()
        .map(|order| {
            Ok(OrderSummary {
                id: crate::models::shopify::ShopifyId::new(order.id)
                    .and_then(|id| id.number("Order"))?,
                number: order.number,
                full_name: order.customer.and_then(|customer| customer.full_name()),
                email: order.email,
                shipping_country_code: order
                    .shipping_address
                    .and_then(|address| address.country_code_v2),
            })
        })
        .collect::<Result<Vec<_>, ClientError>>()
        .map_err(|error| upstream(error))?;
    Ok(ListOrdersResponse {
        orders,
        page_info: result.page_info,
    })
}

pub(super) async fn edit_order_tags(
    client: &crate::client::shopify::ShopifyClient,
    id: &str,
    tags: &[&str],
    action: crate::client::shopify::TagAction,
) -> Result<(), AppError> {
    use crate::{
        client::{ClientErrorKind, shopify::TagAction},
        models::shopify::ShopifyId,
    };
    let id = ShopifyId::new(id.to_owned()).map_err(|error| upstream(error))?;
    let stored = client
        .order_tags(&id)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| {
            AppError::new(axum::http::StatusCode::NOT_FOUND, "Shopify order not found")
        })?;
    if stored.id.as_ref() != id.as_ref() {
        return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
            "tag lookup returned a different order",
        ))));
    }
    let mut edits: Vec<&str> = Vec::new();
    match action {
        TagAction::Add => {
            for tag in tags {
                if !stored
                    .tags
                    .iter()
                    .any(|stored| stored.eq_ignore_ascii_case(tag))
                    && !edits.iter().any(|added| added.eq_ignore_ascii_case(tag))
                {
                    edits.push(tag);
                }
            }
        }
        TagAction::Remove => {
            edits.extend(
                stored
                    .tags
                    .iter()
                    .filter(|stored| tags.iter().any(|tag| stored.eq_ignore_ascii_case(tag)))
                    .map(String::as_str),
            );
        }
    }
    if !edits.is_empty() {
        client
            .edit_order_tags(id.as_ref(), &edits, action)
            .await
            .map_err(|error| upstream(error))?;
    }
    Ok(())
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

pub mod fulfillment;
pub mod shipping_label;

pub mod reship;

#[cfg(test)]
#[path = "../tests/services/orders.rs"]
mod tests;
