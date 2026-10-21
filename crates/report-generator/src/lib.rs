pub mod accounting;
pub mod csv;

#[derive(Clone, PartialEq)]
pub struct Table {
    pub headers: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}
