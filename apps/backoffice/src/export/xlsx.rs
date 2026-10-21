use rust_xlsxwriter::Workbook;

pub fn encode_table_xlsx(
    headers: &[&str],
    rows: &[Vec<String>],
    worksheet_name: &str,
) -> Result<Vec<u8>, String> {
    validate_row_count(rows.len())?;
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet
        .set_name(worksheet_name)
        .map_err(|error| error.to_string())?;
    for (column, header) in headers.iter().enumerate() {
        sheet
            .write_string(
                0,
                u16::try_from(column).map_err(|error| error.to_string())?,
                *header,
            )
            .map_err(|error| error.to_string())?;
    }
    for (row_index, row) in rows.iter().enumerate() {
        let row_index = u32::try_from(row_index + 1).map_err(|error| error.to_string())?;
        for (column, value) in row.iter().enumerate() {
            if !value.is_empty() {
                sheet
                    .write_string(
                        row_index,
                        u16::try_from(column).map_err(|error| error.to_string())?,
                        value,
                    )
                    .map_err(|error| format!("Unable to export XLSX: {error}"))?;
            }
        }
    }
    workbook
        .save_to_buffer()
        .map_err(|error| format!("Unable to export XLSX: {error}"))
}

fn validate_row_count(count: usize) -> Result<(), String> {
    if count > 1_048_575 {
        return Err("XLSX supports at most 1,048,575 rows plus the header".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/export/xlsx.rs"]
mod tests;
