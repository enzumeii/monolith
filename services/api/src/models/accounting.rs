use chrono::{DateTime, FixedOffset};

#[derive(Clone, Debug)]
pub struct AccountingDateRange {
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
}
