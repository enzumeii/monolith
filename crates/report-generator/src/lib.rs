pub mod accounting;
pub mod csv;
pub mod inventory;

pub const CSV_CONTENT_TYPE: &str = "text/csv; charset=utf-8";

#[derive(Clone, PartialEq)]
pub struct Table {
    pub headers: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}
