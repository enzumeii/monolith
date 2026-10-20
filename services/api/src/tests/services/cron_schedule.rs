#![allow(clippy::unwrap_used)]

use crate::services::cron_schedule::{
    CronScheduleError, TransactionPeriodMode, describe, next_occurrence, next_occurrences,
    normalize_expression, transaction_window,
};
use chrono::{DateTime, NaiveDateTime, Utc};

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn next_occurrence_uses_bangkok_wall_clock_time() {
    let next = next_occurrence("0 9 * * *", utc("2026-10-20T01:00:00Z")).unwrap();

    assert_eq!(next, utc("2026-10-20T02:00:00Z"));
}

#[test]
fn next_occurrence_is_strictly_after_the_input_instant() {
    let next = next_occurrence("0 9 * * *", utc("2026-10-20T02:00:00Z")).unwrap();

    assert_eq!(next, utc("2026-10-21T02:00:00Z"));
}

#[test]
fn next_occurrence_rejects_non_five_field_cron() {
    let result = next_occurrence("0 0 9 * * *", utc("2026-10-20T00:00:00Z"));

    assert_eq!(result, Err(CronScheduleError::InvalidExpression));
}

#[test]
fn next_occurrence_rejects_invalid_and_impossible_cron() {
    assert_eq!(
        next_occurrence("60 9 * * *", utc("2026-10-20T00:00:00Z")),
        Err(CronScheduleError::InvalidExpression)
    );
    assert_eq!(
        next_occurrence("0 0 31 2 *", utc("2026-10-20T00:00:00Z")),
        Err(CronScheduleError::NoFutureOccurrence)
    );
}

#[test]
fn expressions_are_canonicalized_and_described() {
    let expression = normalize_expression("  0   9 * * * ").unwrap();

    assert_eq!(expression, "0 9 * * *");
    assert_ne!(describe(&expression).unwrap(), "");
}

#[test]
fn next_occurrences_returns_the_requested_number_in_bangkok_time() {
    let occurrences = next_occurrences("0 9 * * *", utc("2026-10-20T01:00:00Z"), 2).unwrap();

    assert_eq!(occurrences.len(), 2);
    assert_eq!(occurrences[0], utc("2026-10-20T02:00:00Z"));
    assert_eq!(occurrences[1], utc("2026-10-21T02:00:00Z"));
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
