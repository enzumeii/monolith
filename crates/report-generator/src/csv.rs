#[must_use]
pub fn encode_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut csv = format!(
        "{}\r\n",
        headers
            .iter()
            .map(|value| escape(value, false))
            .collect::<Vec<_>>()
            .join(",")
    );
    for row in rows {
        csv.push_str(
            &row.iter()
                .enumerate()
                .map(|(column, value)| escape(value, numeric_column(headers, column)))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push_str("\r\n");
    }
    csv
}

fn escape(value: &str, numeric: bool) -> String {
    let numeric = numeric && value.parse::<f64>().is_ok_and(f64::is_finite);
    let guarded;
    let value = if !numeric
        && (value.starts_with(['\t', '\r']) || value.trim_start().starts_with(['=', '+', '-', '@']))
    {
        guarded = format!("'{value}");
        &guarded
    } else {
        value
    };

    if value.contains([',', '\"', '\n', '\r']) {
        format!("\"{}\"", value.replace('\"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn numeric_column(headers: &[&str], column: usize) -> bool {
    headers.get(column).is_some_and(|header| {
        matches!(
            *header,
            "cost"
                | "weight"
                | "selling_price"
                | "quantity"
                | "total_selling_price"
                | "total_weight"
                | "shopify_order_id"
                | "shopify_order_number"
                | "customer_phone"
                | "exchange_rate"
                | "net_paid"
                | "fee"
                | "vat"
                | "gross_paid"
                | "shipping_fee"
                | "price"
        )
    })
}
#[cfg(test)]
#[path = "tests/csv.rs"]
mod tests;
