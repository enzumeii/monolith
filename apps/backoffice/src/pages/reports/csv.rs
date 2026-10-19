use domain::dto::ProductReportRow;
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
pub fn encode(rows: &[ProductReportRow]) -> String {
    let mut csv = format!("{}\r\n", HEADERS.join(","));
    for row in rows {
        let line = cells(row)
            .iter()
            .map(|value| {
                if value.contains([',', '\"', '\n', '\r']) {
                    format!("\"{}\"", value.replace('\"', "\"\""))
                } else {
                    value.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        csv.push_str(&line);
        csv.push_str("\r\n");
    }
    csv
}
#[cfg(test)]
#[path = "../../tests/pages/reports/csv.rs"]
mod tests;
