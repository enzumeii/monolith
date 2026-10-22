use crate::export::{ExportFormat, encode_table_xlsx};
use domain::dto::ProductReportRow;
use report_generator::inventory;

pub fn encode_report(rows: &[ProductReportRow], format: ExportFormat) -> Result<Vec<u8>, String> {
    if format == ExportFormat::Csv {
        return Ok(inventory::encode(rows).into_bytes());
    }
    let rows = rows
        .iter()
        .map(|row| inventory::cells(row).into())
        .collect::<Vec<_>>();
    encode_table_xlsx(&inventory::HEADERS, &rows, "Reports")
}

#[cfg(test)]
#[path = "../../tests/pages/reports/export.rs"]
mod tests;
