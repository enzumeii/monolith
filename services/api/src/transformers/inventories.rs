use crate::models::{products::ReportInput, shopify::Variant};

pub(crate) fn report_input(
    variant: Variant,
    title: &str,
    category: &str,
    quantity: i64,
) -> ReportInput {
    ReportInput {
        title: title.into(),
        variant: Some(variant.title),
        sku: variant.sku,
        category: Some(category.into()),
        price: variant.contextual_pricing.price,
        quantity,
        metadata: Some(variant.inventory_item.metadata),
    }
}
