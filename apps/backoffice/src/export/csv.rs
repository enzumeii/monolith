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
#[path = "../tests/export/csv.rs"]
mod tests;
