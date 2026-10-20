use crate::{
    client::{ClientError, ClientErrorKind},
    models::{
        Sku,
        shopify::{InventoryMetadata, Money},
    },
    utils::numbers::{multiply_number, parse_number},
};
use domain::dto::ProductReportRow;
use serde_json::Number;
pub(crate) struct ReportRow(ProductReportRow);
impl ReportRow {
    pub(crate) fn new(
        title: String,
        variant: Option<String>,
        sku: Option<Sku>,
        category: Option<String>,
        price: Money,
        quantity: i64,
        metadata: Option<InventoryMetadata>,
    ) -> Result<Self, ClientError> {
        let (cost, weight) = metadata
            .map(|item| (item.unit_cost, item.measurement.weight))
            .unwrap_or_default();
        let mut row = ProductReportRow {
            collection: sku.as_ref().and_then(Sku::collection),
            sku: sku.map(Sku::into_string),
            title,
            variant,
            category: category.filter(|value| !value.is_empty()),
            cost: cost
                .as_ref()
                .map(|value| parse_number(&value.amount))
                .transpose()?,
            cost_currency: cost.map(|value| value.currency_code),
            weight: weight
                .as_ref()
                .map(|value| parse_number(&value.value.to_string()))
                .transpose()?,
            unit: weight.map(|value| value.unit),
            selling_price: parse_number(&price.amount)?,
            selling_price_currency: price.currency_code,
            quantity,
            total_selling_price: Number::from(0),
            total_weight: None,
        };
        totals(&mut row)?;
        Ok(Self(row))
    }

    pub(crate) fn sku(&self) -> Option<&str> {
        self.0.sku.as_deref().filter(|sku| !sku.is_empty())
    }
    pub(crate) fn can_merge(&self, other: &Self) -> bool {
        let old = &self.0;
        let row = &other.0;
        self.sku().is_some()
            && self.sku() == other.sku()
            && old.selling_price == row.selling_price
            && old.selling_price_currency == row.selling_price_currency
            && old.category == row.category
            && old.cost == row.cost
            && old.cost_currency == row.cost_currency
            && old.weight == row.weight
            && old.unit == row.unit
    }
    pub(crate) fn add_quantity(&mut self, quantity: i64) -> Result<(), ClientError> {
        self.0.quantity = self.0.quantity.checked_add(quantity).ok_or_else(|| {
            ClientError::new(ClientErrorKind::Protocol("report quantity overflow"))
        })?;
        totals(&mut self.0)
    }
    pub(crate) fn quantity(&self) -> i64 {
        self.0.quantity
    }
    pub(crate) fn into_dto(self) -> ProductReportRow {
        self.0
    }
}
fn totals(row: &mut ProductReportRow) -> Result<(), ClientError> {
    row.total_selling_price = multiply_number(&row.selling_price, row.quantity)?;
    row.total_weight = row
        .weight
        .as_ref()
        .map(|w| multiply_number(w, row.quantity))
        .transpose()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/models/products.rs"]
mod tests;
