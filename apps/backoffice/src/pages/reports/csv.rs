use crate::export::encode_table;
use domain::dto::ProductReportRow;

pub const HEADERS: [&str; 14] = report_export::INVENTORY_HEADERS;

pub fn cells(row: &ProductReportRow) -> [String; 14] {
    report_export::product_cells(row)
        .try_into()
        .expect("inventory CSV schema has fourteen columns")
}

pub fn encode(rows: &[ProductReportRow]) -> String {
    let rows = rows.iter().map(|row| cells(row).into()).collect::<Vec<_>>();
    encode_table(&HEADERS, &rows)
}

#[cfg(test)]
#[path = "../../tests/pages/reports/csv.rs"]
mod tests;
