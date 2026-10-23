use crate::components::TableData;
#[cfg(test)]
use domain::dto::accounting::{AccountingItem, AccountingTransaction};
pub use report_generator::accounting::{item_table, transaction_table};
pub fn encode_archive(transactions: &TableData, items: &TableData) -> Result<Vec<u8>, String> {
    crate::export::encode_archive(&[
        (
            "transactions.csv",
            crate::export::encode_table(transactions.headers, &transactions.rows).into_bytes(),
        ),
        (
            "items.csv",
            crate::export::encode_table(items.headers, &items.rows).into_bytes(),
        ),
    ])
}

#[cfg(test)]
#[path = "../../../tests/pages/reports/accounting/export.rs"]
mod tests;
