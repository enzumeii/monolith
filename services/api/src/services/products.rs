use crate::{
    client::{ClientError, shopify::ShopifyClient},
    config::Settings,
    error::AppError,
    models::shopify::{
        InventoryMetadata, ListProductsRequest, Money, OrderLinesRequest, OrdersByTagRequest,
        ProductStatus,
    },
    validations::products::ReportSource,
};
use domain::dto::{ProductReportRow, ProductsResponse};
use rust_decimal::Decimal;
use serde_json::Number;
use std::{collections::HashMap, str::FromStr};

pub async fn product_report(
    settings: &Settings,
    source: ReportSource,
) -> Result<ProductsResponse, AppError> {
    let client = shopify_client(settings)?;
    let mut products = Vec::new();
    match source {
        ReportSource::Inventory(location_id) => {
            if !client
                .location_exists(&location_id)
                .await
                .map_err(upstream)?
            {
                return Err(AppError::NotFound.at(module_path!()));
            }
            let catalog = client
                .products_with_variants(&ListProductsRequest {
                    location_id,
                    statuses: vec![ProductStatus::Active],
                })
                .await
                .map_err(upstream)?;
            for product in catalog {
                for variant in product.variants.nodes {
                    let item = variant.inventory_item;
                    let Some(level) = item.inventory_level else {
                        continue;
                    };
                    let quantity = level
                        .quantities
                        .iter()
                        .find(|q| q.name == "on_hand")
                        .ok_or_else(|| protocol("on_hand quantity is missing"))?
                        .quantity;
                    products.push(report_row(
                        product.title.clone(),
                        Some(variant.title),
                        variant.sku,
                        Some(product.product_type.clone()),
                        variant.contextual_pricing.price,
                        quantity,
                        Some(item.metadata),
                    )?);
                }
            }
        }
        ReportSource::OrderTag(tag) => {
            let mut by_sku = HashMap::new();
            let orders = client
                .orders_by_tag(&OrdersByTagRequest { tag })
                .await
                .map_err(upstream)?;
            for order in orders {
                let lines = client
                    .tagged_order_lines(&OrderLinesRequest { order_id: order.id })
                    .await
                    .map_err(upstream)?;
                for line in lines {
                    let row = report_row(
                        line.title,
                        line.variant_title,
                        line.sku,
                        line.product.map(|p| p.product_type),
                        line.original_unit_price_set.shop_money,
                        line.quantity,
                        line.variant.map(|v| v.inventory_item),
                    )?;
                    merge(&mut products, &mut by_sku, row)?;
                }
            }
        }
    }
    Ok(ProductsResponse { products })
}

fn report_row(
    title: String,
    variant: Option<String>,
    sku: Option<String>,
    category: Option<String>,
    price: Money,
    quantity: i64,
    metadata: Option<InventoryMetadata>,
) -> Result<ProductReportRow, AppError> {
    let (cost, weight) = metadata
        .map(|item| (item.unit_cost, item.measurement.weight))
        .unwrap_or_default();
    let mut row = ProductReportRow {
        collection: collection(sku.as_deref()),
        sku,
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
    Ok(row)
}

fn shopify_client(settings: &Settings) -> Result<ShopifyClient, AppError> {
    let config = settings
        .client
        .as_ref()
        .and_then(|c| c.shopify.as_ref())
        .ok_or(AppError::ReportUnavailable(None).at(module_path!()))?;
    ShopifyClient::new(config)
        .map_err(|error| AppError::ReportUnavailable(Some(error)).at("api::client::shopify"))
}

fn upstream(error: ClientError) -> AppError {
    AppError::ReportUpstream(error).at("api::client::shopify")
}
fn protocol(message: &'static str) -> AppError {
    AppError::ReportUpstream(ClientError::Protocol(message)).at(module_path!())
}
fn number(raw: &str) -> Result<Number, AppError> {
    let value = if let Some((base, _)) = raw.split_once(['e', 'E']) {
        Decimal::from_str_exact(base).map_err(|_| protocol("invalid report decimal"))?;
        Decimal::from_scientific(raw)
    } else {
        Decimal::from_str_exact(raw)
    }
    .map_err(|_| protocol("invalid report decimal"))?;
    Number::from_str(&value.normalize().to_string()).map_err(|_| protocol("invalid JSON decimal"))
}
fn total(value: &Number, quantity: i64) -> Result<Number, AppError> {
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
fn merge(
    rows: &mut Vec<ProductReportRow>,
    by_sku: &mut HashMap<String, Vec<usize>>,
    row: ProductReportRow,
) -> Result<(), AppError> {
    let candidates = row
        .sku
        .as_ref()
        .filter(|sku| !sku.is_empty())
        .map(|sku| by_sku.entry(sku.clone()).or_default());
    let existing = candidates.as_ref().and_then(|indices| {
        indices.iter().copied().find(|&index| {
            let old = &rows[index];
            old.selling_price == row.selling_price
                && old.selling_price_currency == row.selling_price_currency
                && old.category == row.category
                && old.cost == row.cost
                && old.cost_currency == row.cost_currency
                && old.weight == row.weight
                && old.unit == row.unit
        })
    });
    if let Some(index) = existing {
        let old = &mut rows[index];
        old.quantity = old
            .quantity
            .checked_add(row.quantity)
            .ok_or_else(|| protocol("report quantity overflow"))?;
        totals(old)
    } else {
        if let Some(indices) = candidates {
            indices.push(rows.len());
        }
        rows.push(row);
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/services/products.rs"]
mod tests;
