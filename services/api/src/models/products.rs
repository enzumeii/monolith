use crate::{
    client::{ClientError, ClientErrorKind},
    models::shopify::{InventoryMetadata, Money, Sku},
};
use domain::dto::ProductReportRow;
use rust_decimal::Decimal;
use serde_json::Number;
use std::str::FromStr;
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
                .map(|value| number(&value.amount))
                .transpose()?,
            cost_currency: cost.map(|value| value.currency_code),
            weight: weight
                .as_ref()
                .map(|value| number(&value.value.to_string()))
                .transpose()?,
            unit: weight.map(|value| value.unit),
            selling_price: number(&price.amount)?,
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
        self.0.quantity = self
            .0
            .quantity
            .checked_add(quantity)
            .ok_or_else(|| protocol("report quantity overflow"))?;
        totals(&mut self.0)
    }
    pub(crate) fn quantity(&self) -> i64 {
        self.0.quantity
    }
    pub(crate) fn into_dto(self) -> ProductReportRow {
        self.0
    }
}
fn number(raw: &str) -> Result<Number, ClientError> {
    let value = if let Some((base, _)) = raw.split_once(['e', 'E']) {
        Decimal::from_str_exact(base).map_err(|_| protocol("invalid report decimal"))?;
        Decimal::from_scientific(raw)
    } else {
        Decimal::from_str_exact(raw)
    }
    .map_err(|_| protocol("invalid report decimal"))?;
    Number::from_str(&value.normalize().to_string()).map_err(|_| protocol("invalid JSON decimal"))
}
fn total(value: &Number, quantity: i64) -> Result<Number, ClientError> {
    let value = Decimal::from_str_exact(&value.to_string())
        .map_err(|_| protocol("invalid report decimal"))?
        .normalize();
    let mut mantissa = value.mantissa();
    let mut multiplier = i128::from(quantity);
    let mut scale = value.scale();
    if mantissa == 0 || multiplier == 0 {
        return Ok(Number::from(0));
    }
    // Cancel decimal places before multiplying so a temporary coefficient cannot overflow an exact result.
    while scale > 0 {
        let factors = [(10, 1), (1, 10), (2, 5), (5, 2)]
            .into_iter()
            .find(|(a, b)| mantissa % a == 0 && multiplier % b == 0);
        let Some((a, b)) = factors else {
            break;
        };
        mantissa /= a;
        multiplier /= b;
        scale -= 1;
    }
    let mantissa = mantissa
        .checked_mul(multiplier)
        .ok_or_else(|| protocol("report total overflow"))?;
    let result = Decimal::try_from_i128_with_scale(mantissa, scale)
        .map_err(|_| protocol("report total overflow"))?;
    number(&result.to_string())
}
fn totals(row: &mut ProductReportRow) -> Result<(), ClientError> {
    row.total_selling_price = total(&row.selling_price, row.quantity)?;
    row.total_weight = row
        .weight
        .as_ref()
        .map(|w| total(w, row.quantity))
        .transpose()?;
    Ok(())
}

#[track_caller]
fn protocol(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(message))
}
#[cfg(test)]
#[path = "../tests/models/products.rs"]
mod tests;
