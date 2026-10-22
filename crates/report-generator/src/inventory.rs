use crate::csv::encode_table;
use domain::dto::ProductReportRow;

const NUMERIC_COLUMNS: &[usize] = &[5, 7, 9, 11, 12, 13];

pub const HEADERS: [&str; 14] = [
    "category",
    "collection",
    "sku",
    "title",
    "variant",
    "cost",
    "cost_currency",
    "weight",
    "unit",
    "selling_price",
    "selling_price_currency",
    "quantity",
    "total_selling_price",
    "total_weight",
];

pub fn cells(row: &ProductReportRow) -> [String; 14] {
    [
        row.category.clone().unwrap_or_default(),
        row.collection.clone().unwrap_or_default(),
        row.sku.clone().unwrap_or_default(),
        row.title.clone(),
        row.variant.clone().unwrap_or_default(),
        row.cost
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        row.cost_currency.clone().unwrap_or_default(),
        row.weight
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        row.unit.clone().unwrap_or_default(),
        row.selling_price.to_string(),
        row.selling_price_currency.clone(),
        row.quantity.to_string(),
        row.total_selling_price.to_string(),
        row.total_weight
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
    ]
}

#[must_use]
pub fn encode(rows: &[ProductReportRow]) -> String {
    let rows = rows.iter().map(|row| cells(row).into()).collect::<Vec<_>>();
    encode_table(&HEADERS, &rows, NUMERIC_COLUMNS)
}

#[cfg(test)]
#[path = "tests/inventory.rs"]
mod tests;
