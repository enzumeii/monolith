#[must_use]
pub fn encode_table(headers: &[&str], rows: &[Vec<String>], numeric_columns: &[usize]) -> String {
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
                .map(|(column, value)| escape(value, numeric_columns.contains(&column)))
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

#[cfg(test)]
#[path = "tests/csv.rs"]
mod tests;
