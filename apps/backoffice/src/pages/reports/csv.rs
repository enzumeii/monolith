#[cfg(test)]
use domain::dto::ProductReportRow;
pub use report_generator::csv::{HEADERS, cells, encode, encode_table};
#[cfg(test)]
#[path = "../../tests/pages/reports/csv.rs"]
mod tests;
