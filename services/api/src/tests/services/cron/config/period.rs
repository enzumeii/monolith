#![allow(clippy::unwrap_used)]
use super::{TransactionPeriodMode, transaction_window};
use chrono::{DateTime, NaiveDateTime, Utc};
fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
#[test]
fn previous_month_window_uses_the_nominal_bangkok_occurrence() {
    let window = transaction_window(
        TransactionPeriodMode::PreviousMonth,
        utc("2026-11-01T17:00:00Z"),
    )
    .unwrap();

    assert_eq!(
        window.start,
        NaiveDateTime::parse_from_str("2026-09-30 17:00:00", "%F %T").unwrap()
    );
    assert_eq!(
        window.end_exclusive,
        NaiveDateTime::parse_from_str("2026-10-31 17:00:00", "%F %T").unwrap()
    );
}

#[test]
fn first_half_window_uses_the_sixteenth_occurrence_and_exclusive_end() {
    let window = transaction_window(
        TransactionPeriodMode::FirstHalfMonth,
        utc("2026-10-15T17:00:00Z"),
    )
    .unwrap();

    assert_eq!(
        window.start,
        NaiveDateTime::parse_from_str("2026-09-30 17:00:00", "%F %T").unwrap()
    );
    assert_eq!(
        window.end_exclusive,
        NaiveDateTime::parse_from_str("2026-10-15 17:00:00", "%F %T").unwrap()
    );
}

#[test]
fn previous_month_window_handles_leap_year_and_year_boundary() {
    let leap_year = transaction_window(
        TransactionPeriodMode::PreviousMonth,
        utc("2024-02-29T17:00:00Z"),
    )
    .unwrap();
    let year_boundary = transaction_window(
        TransactionPeriodMode::PreviousMonth,
        utc("2027-01-01T17:00:00Z"),
    )
    .unwrap();

    assert_eq!(
        leap_year.start.format("%F %T").to_string(),
        "2024-01-31 17:00:00"
    );
    assert_eq!(
        leap_year.end_exclusive.format("%F %T").to_string(),
        "2024-02-29 17:00:00"
    );
    assert_eq!(
        year_boundary.start.format("%F %T").to_string(),
        "2026-11-30 17:00:00"
    );
    assert_eq!(
        year_boundary.end_exclusive.format("%F %T").to_string(),
        "2026-12-31 17:00:00"
    );
}
