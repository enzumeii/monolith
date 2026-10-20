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
    let rows = rows.iter().map(|row| cells(row).into()).collect::<Vec<_>>();
    encode_table(&HEADERS, &rows)
}

pub fn encode_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut csv = format!(
        "{}\r\n",
        headers
            .iter()
            .map(|value| escape(value))
            .collect::<Vec<_>>()
            .join(",")
    );
    for row in rows {
        csv.push_str(
            &row.iter()
                .map(|value| escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    csv
}

fn escape(value: &str) -> String {
    if value.contains([',', '\"', '\n', '\r']) {
        format!("\"{}\"", value.replace('\"', "\"\""))
    } else {
        value.to_owned()
    }
}
#[cfg(test)]
#[path = "../../tests/pages/reports/csv.rs"]
mod tests;
