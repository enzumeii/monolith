use super::csv;
use crate::export::{ExportFormat, encode_table_xlsx};
use domain::dto::ProductReportRow;

pub fn encode_report(rows: &[ProductReportRow], format: ExportFormat) -> Result<Vec<u8>, String> {
    if format == ExportFormat::Csv {
        return Ok(csv::encode(rows).into_bytes());
    }
    let rows = rows
        .iter()
        .map(|row| csv::cells(row).into())
        .collect::<Vec<_>>();
    encode_table_xlsx(&csv::HEADERS, &rows, "Reports")
}

#[cfg(test)]
#[path = "../../tests/pages/reports/export.rs"]
mod tests;
