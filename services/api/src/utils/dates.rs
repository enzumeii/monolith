use crate::client::{ClientError, ClientErrorKind};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};

pub(crate) fn parse_rfc3339(value: &str) -> Result<DateTime<FixedOffset>, ClientError> {
    DateTime::parse_from_rfc3339(value).map_err(|_| invalid_date())
}

pub(crate) fn iso_utc(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

#[track_caller]
fn invalid_date() -> ClientError {
    ClientError::new(ClientErrorKind::Protocol("invalid accounting date"))
}

#[cfg(test)]
#[path = "../tests/utils/dates.rs"]
mod tests;
