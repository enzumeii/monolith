mod browser;
mod csv;

pub(crate) use browser::{download, encoded_bytes};
pub(crate) use csv::encode_table;
use rust_xlsxwriter::Workbook;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Xlsx,
}
impl ExportFormat {
    pub fn from_filename(name: &str) -> Result<Self, String> {
        valid_name(name)?;
        match name.trim().rsplit_once('.') {
            Some((base, ext)) if !base.is_empty() && ext.eq_ignore_ascii_case("csv") => {
                Ok(Self::Csv)
            }
            Some((base, ext)) if !base.is_empty() && ext.eq_ignore_ascii_case("xlsx") => {
                Ok(Self::Xlsx)
            }
            _ => Err("Use a filename ending in .csv or .xlsx".into()),
        }
    }

    pub fn filename(self, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let name = name.trim();
        let base = if name.contains('.') {
            Self::from_filename(name)?;
            name.rsplit_once('.').map_or(name, |(base, _)| base)
        } else {
            name
        };
        Ok(format!("{base}.{}", self.extension()))
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Csv => "text/csv;charset=utf-8",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }
}

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

fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err("Enter a filename without folders or control characters".into());
    }
    Ok(())
}
fn validate_row_count(count: usize) -> Result<(), String> {
    if count > 1_048_575 {
        return Err("XLSX supports at most 1,048,575 rows plus the header".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "tests/export.rs"]
pub(crate) mod tests;
