use super::{InventoryLevel, TaggedOrderLineItem, Variant};
use crate::{client::ClientError, models::products::ReportRow};
impl Variant {
    pub(crate) fn into_report_row(
        self,
        product_title: &str,
        product_type: &str,
    ) -> Result<Option<ReportRow>, ClientError> {
        let Some(level) = self
            .inventory_item
            .inventory_level
            .filter(|level| level.is_active)
        else {
            return Ok(None);
        };
        Ok(Some(ReportRow::new(
            product_title.into(),
            Some(self.title),
            self.sku,
            Some(product_type.into()),
            self.contextual_pricing.price,
            level.on_hand()?,
            Some(self.inventory_item.metadata),
        )?))
    }
}
impl TaggedOrderLineItem {
    pub(crate) fn into_report_row(self) -> Result<ReportRow, ClientError> {
        ReportRow::new(
            self.title,
            self.variant_title,
            self.sku,
            self.product.map(|p| p.product_type),
            self.original_unit_price_set.shop_money,
            self.quantity,
            self.variant.map(|v| v.inventory_item),
        )
    }
}
impl InventoryLevel {
    pub(crate) fn on_hand(&self) -> Result<i64, ClientError> {
        self.quantities
            .iter()
            .find(|q| q.name == "on_hand")
            .map(|q| q.quantity)
            .ok_or_else(|| {
                ClientError::Protocol("on_hand quantity is missing")
                    .at("api::models::shopify::products")
            })
    }
}
#[cfg(test)]
#[path = "../../tests/models/shopify/products.rs"]
mod tests;
