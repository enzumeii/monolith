use super::super::{
    common::{shopify_client, upstream},
    pagination::for_each_page,
};
use crate::{
    client::{ClientError, ClientErrorKind},
    error::AppError,
    models::{fulfillment::resource_number, shopify::PageRequest},
    state::AppState,
};
use domain::dto::orders::{ListOrdersResponse, OrderPageInfo, OrderSummary, ShippingLabels};

pub async fn list(
    state: &AppState,
    search: String,
    page: PageRequest,
) -> Result<ListOrdersResponse, AppError> {
    let result = shopify_client(state)?
        .list_orders(&search, &page)
        .await
        .map_err(|error| upstream(error))?;
    result
        .page_info
        .validate_after(page.after.as_deref())
        .map_err(|error| upstream(error))?;
    let orders = result
        .nodes
        .into_iter()
        .map(|order| {
            Ok(OrderSummary {
                id: resource_number(&order.id, "Order")?,
                number: order.number,
            })
        })
        .collect::<Result<Vec<_>, ClientError>>()
        .map_err(|error| upstream(error))?;
    Ok(ListOrdersResponse {
        orders,
        page_info: OrderPageInfo {
            has_next_page: result.page_info.has_next_page,
            end_cursor: result.page_info.end_cursor,
        },
    })
}

pub async fn labels(state: &AppState) -> Result<ShippingLabels, AppError> {
    let client = shopify_client(state)?;
    let mut orders = Vec::new();
    for_each_page(
        async |after| client.shipping_orders(after).await,
        |page| {
            orders.extend(page);
            Ok(())
        },
    )
    .await
    .map_err(|error| upstream(error))?;
    let mut labels = ShippingLabels::default();
    let mut tags = Vec::new();
    for mut order in orders {
        resource_number(&order.id, "Order").map_err(|error| upstream(error))?;
        let id = order.id.clone();
        let items = super::super::pagination::collect_connection(order.line_items, async |after| {
            let result = client.shipping_order_items(&id, after).await?;
            let next = result.order.ok_or_else(|| {
                ClientError::new(ClientErrorKind::Protocol("shipping order disappeared"))
            })?;
            ensure_same_order(&id, &next.id)?;
            Ok(next.line_items)
        })
        .await
        .map_err(|error| upstream(error))?;
        order.line_items = complete(items);
        let methods =
            super::super::pagination::collect_connection(order.shipping_lines, async |after| {
                let result = client.shipping_order_methods(&id, after).await?;
                let next = result.order.ok_or_else(|| {
                    ClientError::new(ClientErrorKind::Protocol("shipping order disappeared"))
                })?;
                ensure_same_order(&id, &next.id)?;
                Ok(next.shipping_lines)
            })
            .await
            .map_err(|error| upstream(error))?;
        order.shipping_lines = complete(methods);
        let courier = super::shipping::append_label(
            order,
            state.settings.shipping_sender.as_ref(),
            &mut labels,
        )?;
        tags.push((id, courier));
    }
    for (id, courier) in tags {
        let updated = client
            .add_order_tags(&id, &["SHIPPING", courier])
            .await
            .map_err(|error| upstream(error))?;
        ensure_same_order(&id, &updated).map_err(|error| upstream(error))?;
        let updated = client
            .remove_order_tags(&id, &["PACKING"])
            .await
            .map_err(|error| upstream(error))?;
        ensure_same_order(&id, &updated).map_err(|error| upstream(error))?;
    }
    Ok(labels)
}
pub(super) fn ensure_same_order(expected: &str, actual: &str) -> Result<(), ClientError> {
    if expected != actual {
        return Err(ClientError::new(ClientErrorKind::Protocol(
            "mutation returned a different order",
        )));
    }
    Ok(())
}

fn complete<T>(nodes: Vec<T>) -> crate::models::shopify::Connection<T> {
    crate::models::shopify::Connection {
        nodes,
        page_info: crate::models::shopify::PageInfo {
            has_next_page: false,
            end_cursor: None,
        },
    }
}
