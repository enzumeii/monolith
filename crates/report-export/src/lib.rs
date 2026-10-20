use domain::dto::ProductReportRow;

pub const INVENTORY_HEADERS: [&str; 14] = [
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

#[must_use]
pub fn spreadsheet_safe_text(value: &str) -> String {
    if value
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r'])
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

#[must_use]
pub fn encode_csv(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut csv = String::new();
    csv.push_str(
        &headers
            .iter()
            .map(|v| csv_cell(v))
            .collect::<Vec<_>>()
            .join(","),
    );
    csv.push_str("\r\n");
    for row in rows {
        csv.push_str(
            &row.iter()
                .map(|v| csv_cell(v))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    csv
}

pub fn encode_product_report(rows: &[ProductReportRow]) -> String {
    let rows = rows.iter().map(product_cells).collect::<Vec<_>>();
    encode_csv(&INVENTORY_HEADERS, &rows)
}

#[must_use]
pub fn product_cells(row: &ProductReportRow) -> Vec<String> {
    vec![
        row.category.clone().unwrap_or_default(),
        row.collection.clone().unwrap_or_default(),
        row.sku.clone().unwrap_or_default(),
        row.title.clone(),
        row.variant.clone().unwrap_or_default(),
        numeric(row.cost.as_ref()),
        row.cost_currency.clone().unwrap_or_default(),
        numeric(row.weight.as_ref()),
        row.unit.clone().unwrap_or_default(),
        row.selling_price.to_string(),
        row.selling_price_currency.clone(),
        row.quantity.to_string(),
        row.total_selling_price.to_string(),
        numeric(row.total_weight.as_ref()),
    ]
}

fn numeric(value: Option<&serde_json::Number>) -> String {
    value.map(ToString::to_string).unwrap_or_default()
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_csv_and_makes_formula_text_safe_without_changing_numbers() {
        let formula_text = spreadsheet_safe_text("=SUM(A1)");
        assert_eq!(
            encode_csv(&["name", "amount"], &[vec![formula_text, "-4.2".into()]]),
            "name,amount\r\n'=SUM(A1),-4.2\r\n"
        );
        assert_eq!(
            encode_csv(&["name"], &[vec!["a,\"b\"".into()]]),
            "name\r\n\"a,\"\"b\"\"\"\r\n"
        );
    }
}
