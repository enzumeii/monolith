use crate::{
    client::{ClientError, shopify::ShopifyClient},
    config::Settings,
    error::AppError,
    models::shopify::{
        ListProductsRequest, OrderLinesRequest, OrdersByTagRequest, ProductStatus,
        report::{Measurement, ReportInventoryItem},
    },
    validations::products::ReportSource,
};
use domain::dto::{ProductReportRow, ProductsResponse};
use rust_decimal::Decimal;
use serde_json::Number;
use std::str::FromStr;

pub async fn product_report(
    settings: &Settings,
    source: ReportSource,
) -> Result<ProductsResponse, AppError> {
    let config = settings
        .client
        .as_ref()
        .and_then(|c| c.shopify.as_ref())
        .ok_or(AppError::ReportUnavailable.at(module_path!()))?;
    let client =
        ShopifyClient::new(config).map_err(|_| AppError::ReportUnavailable.at(module_path!()))?;
    let mut products = Vec::new();
    match source {
        ReportSource::Inventory(location_id) => {
            let catalog = client
                .inventory_report_products(&ListProductsRequest {
                    location_id,
                    statuses: vec![ProductStatus::Active],
                })
                .await
                .map_err(upstream)?
                .ok_or(AppError::NotFound.at(module_path!()))?;
            for product in catalog {
                for variant in product.variants.nodes {
                    let Some(level) = variant.inventory_item.inventory_level.as_ref() else {
                        continue;
                    };
                    let quantity = level
                        .quantities
                        .iter()
                        .find(|q| q.name == "on_hand")
                        .ok_or_else(|| protocol("on_hand quantity is missing"))?
                        .quantity;
                    let mut row = ProductReportRow {
                        category: nonempty(&product.product_type),
                        collection: collection(variant.sku.as_deref()),
                        sku: variant.sku,
                        title: product.title.clone(),
                        variant: Some(variant.title),
                        cost: None,
                        cost_currency: None,
                        weight: None,
                        unit: None,
                        selling_price: number(&variant.contextual_pricing.price.amount)?,
                        selling_price_currency: variant.contextual_pricing.price.currency_code,
                        quantity,
                        total_selling_price: Number::from(0),
                        total_weight: None,
                    };
                    inventory_metadata(&mut row, &variant.inventory_item)?;
                    totals(&mut row)?;
                    products.push(row);
                }
            }
        }
        ReportSource::OrderTag(tag) => {
            let orders = client
                .orders_by_tag(&OrdersByTagRequest { tag })
                .await
                .map_err(upstream)?;
            for order in orders {
                let lines = client
                    .report_tagged_order_lines(&OrderLinesRequest { order_id: order.id })
                    .await
                    .map_err(upstream)?;
                for line in lines {
                    let mut row = ProductReportRow {
                        category: line
                            .product
                            .as_ref()
                            .and_then(|p| nonempty(&p.product_type)),
                        collection: collection(line.sku.as_deref()),
                        sku: line.sku,
                        title: line.title,
                        variant: line.variant_title,
                        cost: None,
                        cost_currency: None,
                        weight: None,
                        unit: None,
                        selling_price: number(&line.original_unit_price_set.shop_money.amount)?,
                        selling_price_currency: line
                            .original_unit_price_set
                            .shop_money
                            .currency_code,
                        quantity: line.quantity,
                        total_selling_price: Number::from(0),
                        total_weight: None,
                    };
                    if let Some(variant) = line.variant {
                        cost_metadata(&mut row, variant.inventory_item.unit_cost.as_ref())?;
                        weight_metadata(&mut row, &variant.inventory_item.measurement)?;
                    }
                    merge(&mut products, row)?;
                }
            }
        }
    }
    Ok(ProductsResponse { products })
}

fn upstream(error: ClientError) -> AppError {
    AppError::ReportUpstream(error).at("api::client::shopify")
}
fn protocol(message: &'static str) -> AppError {
    upstream(ClientError::Protocol(message))
}
fn number(raw: &str) -> Result<Number, AppError> {
    let value = Decimal::from_str_exact(raw).map_err(|_| protocol("invalid report decimal"))?;
    Number::from_str(&value.normalize().to_string()).map_err(|_| protocol("invalid JSON decimal"))
}
fn decimal(value: &Number) -> Result<Decimal, AppError> {
    Decimal::from_str_exact(&value.to_string()).map_err(|_| protocol("invalid report decimal"))
}
fn total(value: &Number, quantity: i64) -> Result<Number, AppError> {
    let value = decimal(value)?;
    let mantissa = value
        .mantissa()
        .checked_mul(i128::from(quantity))
        .ok_or_else(|| protocol("report total overflow"))?;
    let result = Decimal::try_from_i128_with_scale(mantissa, value.scale())
        .map_err(|_| protocol("report total overflow"))?;
    number(&result.to_string())
}
fn totals(row: &mut ProductReportRow) -> Result<(), AppError> {
    row.total_selling_price = total(&row.selling_price, row.quantity)?;
    row.total_weight = row
        .weight
        .as_ref()
        .map(|w| total(w, row.quantity))
        .transpose()?;
    Ok(())
}
fn collection(sku: Option<&str>) -> Option<String> {
    let chars = sku?.chars().skip(3).take(6).collect::<String>();
    (chars.chars().count() == 6).then_some(chars)
}
fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}
fn cost_metadata(
    row: &mut ProductReportRow,
    cost: Option<&crate::models::shopify::Money>,
) -> Result<(), AppError> {
    if let Some(cost) = cost {
        row.cost = Some(number(&cost.amount)?);
        row.cost_currency = Some(cost.currency_code.clone());
    }
    Ok(())
}
fn weight_metadata(row: &mut ProductReportRow, measurement: &Measurement) -> Result<(), AppError> {
    if let Some(weight) = &measurement.weight {
        row.weight = Some(number(&weight.value.to_string())?);
        row.unit = Some(weight.unit.clone());
    }
    Ok(())
}
fn inventory_metadata(
    row: &mut ProductReportRow,
    item: &ReportInventoryItem,
) -> Result<(), AppError> {
    cost_metadata(row, item.unit_cost.as_ref())?;
    weight_metadata(row, &item.measurement)
}
fn merge(rows: &mut Vec<ProductReportRow>, mut row: ProductReportRow) -> Result<(), AppError> {
    let price = decimal(&row.selling_price)?;
    let existing = rows.iter_mut().find(|old| {
        row.sku.as_ref().is_some_and(|sku| !sku.is_empty())
            && old.sku == row.sku
            && decimal(&old.selling_price).ok() == Some(price)
            && old.selling_price_currency == row.selling_price_currency
            && old.category == row.category
            && old.collection == row.collection
            && old.cost == row.cost
            && old.cost_currency == row.cost_currency
            && old.weight == row.weight
            && old.unit == row.unit
    });
    if let Some(old) = existing {
        old.quantity = old
            .quantity
            .checked_add(row.quantity)
            .ok_or_else(|| protocol("report quantity overflow"))?;
        totals(old)
    } else {
        totals(&mut row)?;
        rows.push(row);
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/services/products.rs"]
mod tests;
