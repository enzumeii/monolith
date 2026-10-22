#![allow(clippy::unwrap_used)]

use crate::cron::schedule::{
    CronScheduleError, describe, next_occurrence, next_occurrences, normalize_expression,
};
use chrono::{DateTime, Utc};

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
