use crate::client::pagination::{collect_connection, collect_pages};
use crate::{
    client::{ClientError, shopify::ShopifyClient},
    models::shopify::{
        ActivateInventoryRequest, ActivationData, Connection, InventoryActivationCandidate,
        InventoryData, InventoryDiscoveryRequest, InventoryLevel, ListProductsRequest,
        MutationUserError, Order, OrderLinesData, OrderLinesRequest, OrderTags, OrderUpdateData,
        OrdersByPaymentRequest, OrdersByTagData, OrdersByTagRequest, OrdersData, Product,
        ProductsData, ReplaceOrderTagsRequest, SalesData, SalesOrder, SalesRangeRequest, ShopifyId,
        TaggedOrder, TaggedOrderLineItems,
    },
};

pub async fn list_products(
    client: &ShopifyClient,
    input: &ListProductsRequest,
) -> Result<Vec<Product>, ClientError> {
    collect_pages(async |after| {
        let data: ProductsData = client
            .products_with_variants(input, after.as_deref())
            .await?;
        Ok(data.products)
    })
    .await
}

pub async fn orders_by_payment_id(
    client: &ShopifyClient,
    input: &OrdersByPaymentRequest,
) -> Result<Vec<Order>, ClientError> {
    collect_pages(async |after| {
        let data: OrdersData = client.orders_by_payment(input, after.as_deref()).await?;
        Ok(data.orders)
    })
    .await
}

pub async fn orders_by_tag(
    client: &ShopifyClient,
    input: &OrdersByTagRequest,
) -> Result<Vec<TaggedOrder>, ClientError> {
    let headers = collect_pages(async |after| {
        let data: OrdersByTagData = client.orders_by_tag(input, after.as_deref()).await?;
        Ok(data.orders)
    })
    .await?;
    let mut orders = Vec::with_capacity(headers.len());
    for header in headers {
        let request = OrderLinesRequest {
            order_id: header.id.clone(),
        };
        let lines = collect_connection(async |after| {
            let data = client
                .tagged_order_lines(&request, after.as_deref())
                .await?;
            require_lines(data, &header.id)
        })
        .await?;
        orders.push(header.with_lines(TaggedOrderLineItems { nodes: lines.nodes }));
    }
    Ok(orders)
}

pub async fn sales_in_range(
    client: &ShopifyClient,
    input: &SalesRangeRequest,
) -> Result<Vec<SalesOrder>, ClientError> {
    let headers = collect_pages(async |after| {
        let data: SalesData = client
            .sold_variants_in_range(input, after.as_deref())
            .await?;
        Ok(data.orders)
    })
    .await?;
    let mut orders = Vec::with_capacity(headers.len());
    for header in headers {
        let request = OrderLinesRequest {
            order_id: header.id.clone(),
        };
        let lines = collect_connection(async |after| {
            let data = client.sales_order_lines(&request, after.as_deref()).await?;
            require_lines(data, &header.id)
        })
        .await?;
        orders.push(header.with_lines(lines));
    }
    Ok(orders)
}

pub async fn replace_order_tags(
    client: &ShopifyClient,
    input: &ReplaceOrderTagsRequest,
) -> Result<OrderTags, ClientError> {
    let data: OrderUpdateData = client.replace_order_tags(input).await?;
    check_user_errors(data.order_update.user_errors)?;
    data.order_update
        .order
        .ok_or(ClientError::Protocol("updated order is missing"))
}

pub async fn find_inventory_to_activate(
    client: &ShopifyClient,
    input: &InventoryDiscoveryRequest,
) -> Result<Vec<InventoryActivationCandidate>, ClientError> {
    let levels = collect_pages(async |after| {
        let data: InventoryData = client
            .find_inventory_to_activate(input, after.as_deref())
            .await?;
        Ok(data
            .location
            .ok_or(ClientError::InvalidInput("location was not found"))?
            .inventory_levels)
    })
    .await?;
    let mut candidates = Vec::new();
    for level in levels {
        let on_hand = level
            .quantities
            .iter()
            .find(|q| q.name == "on_hand")
            .ok_or(ClientError::Protocol("on_hand quantity is missing"))?
            .quantity;
        if level.item.tracked && !level.is_active && on_hand > 0 {
            candidates.push(level);
        }
    }
    Ok(candidates)
}

pub async fn activate_inventory(
    client: &ShopifyClient,
    input: &ActivateInventoryRequest,
) -> Result<InventoryLevel, ClientError> {
    let data: ActivationData = client.activate_existing_inventory(input).await?;
    check_user_errors(data.inventory_activate.user_errors)?;
    data.inventory_activate
        .inventory_level
        .ok_or(ClientError::Protocol(
            "activated inventory level is missing",
        ))
}

fn require_lines<T>(data: OrderLinesData<T>, id: &ShopifyId) -> Result<Connection<T>, ClientError> {
    let order = data.order.ok_or(ClientError::Protocol(
        "order is missing during line-item pagination",
    ))?;
    if &order.id != id {
        return Err(ClientError::Protocol(
            "line items belong to a different order",
        ));
    }
    Ok(order.line_items)
}
fn check_user_errors(errors: Vec<MutationUserError>) -> Result<(), ClientError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ClientError::Mutation { errors })
    }
}

#[cfg(test)]
#[path = "../tests/services/shopify.rs"]
mod tests;
