use crate::models::{products::ReportInput, shopify::TaggedOrderLineItem};

pub(crate) fn report_input(line: TaggedOrderLineItem) -> ReportInput {
    ReportInput {
        title: line.title,
        variant: line.variant_title,
        sku: line.sku,
        category: line.product.map(|product| product.product_type),
        price: line.original_unit_price_set.shop_money,
        quantity: line.quantity,
        metadata: line.variant.map(|variant| variant.inventory_item),
    }
}

pub(crate) fn summary(
    order: crate::models::shopify::OrderSummary,
) -> Result<domain::dto::orders::OrderSummary, crate::client::ClientError> {
    Ok(domain::dto::orders::OrderSummary {
        id: crate::models::shopify::ShopifyId::new(order.id).and_then(|id| id.number("Order"))?,
        number: order.number,
        full_name: order.customer.and_then(|customer| customer.full_name()),
        email: order.email,
        shipping_country_code: order
            .shipping_address
            .and_then(|address| address.country_code_v2),
    })
}
