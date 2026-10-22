use std::fmt::Write;

#[must_use]
pub fn encode_table(headers: &[&str], rows: &[Vec<String>], numeric_columns: &[usize]) -> String {
    let mut csv = String::new();
    let _ = write_row(&mut csv, headers, &[]);
    for row in rows {
        let _ = write_row(&mut csv, row, numeric_columns);
    }
    csv
}

pub fn write_row(
    output: &mut impl Write,
    values: impl IntoIterator<Item = impl AsRef<str>>,
    numeric_columns: &[usize],
) -> std::fmt::Result {
    for (column, value) in values.into_iter().enumerate() {
        if column != 0 {
            output.write_char(',')?;
        }
        write_cell(output, value.as_ref(), numeric_columns.contains(&column))?;
    }
    output.write_str("\r\n")
}

fn write_cell(output: &mut impl Write, value: &str, numeric: bool) -> std::fmt::Result {
    let numeric = numeric && value.parse::<f64>().is_ok_and(f64::is_finite);
    let guarded = !numeric
        && (value.starts_with(['\t', '\r'])
            || value.trim_start().starts_with(['=', '+', '-', '@']));
    let quoted = value.contains([',', '\"', '\n', '\r']);
    if quoted {
        output.write_char('"')?;
    }
    if guarded {
        output.write_char('\'')?;
    }
    if quoted {
        for (index, part) in value.split('"').enumerate() {
            if index != 0 {
                output.write_str("\"\"")?;
            }
            output.write_str(part)?;
        }
        output.write_char('"')
    } else {
        output.write_str(value)
    }
}

#[cfg(test)]
#[path = "tests/csv.rs"]
mod tests;
