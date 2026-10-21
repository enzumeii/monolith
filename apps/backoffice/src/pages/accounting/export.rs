use crate::components::TableData;
#[cfg(test)]
use domain::dto::accounting::{AccountingItem, AccountingTransaction};
pub use report_generator::accounting::{item_table, transaction_table};
pub fn encode_archive(transactions: &TableData, items: &TableData) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, table) in [("transactions.csv", transactions), ("items.csv", items)] {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        let csv = crate::export::encode_table(table.headers, &table.rows);
        archive
            .write_all(csv.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "../../tests/pages/accounting/export.rs"]
mod tests;
