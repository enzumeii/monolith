#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{identity, records};
use crate::{models::paypal::TransactionDetail, validations::accounting::AccountingDateRange};
use serde_json::json;

fn detail(id: &str, event: &str, date: &str, currency: &str, amount: &str) -> TransactionDetail {
    serde_json::from_value(json!({"transaction_info":{"transaction_id":id,"transaction_event_code":event,"transaction_amount":{"currency_code":currency,"value":amount},"transaction_initiation_date":date,"transaction_updated_date":date,"transaction_status":"S"}})).unwrap()
}
fn range(start: &str, end: &str) -> AccountingDateRange {
    AccountingDateRange {
        start: start.parse().unwrap(),
        end: end.parse().unwrap(),
    }
}
#[test]
fn payout_bounds_compare_instants_including_offset_edges() {
    let bounds = range("2026-10-01T00:00:00+07:00", "2026-10-31T23:59:59+07:00");
    let rows = records(
        vec![
            detail("before", "T0006", "2026-09-30T16:59:59Z", "THB", "10"),
            detail("start", "T0006", "2026-09-30T17:00:00Z", "THB", "10"),
            detail("october", "T0006", "2026-09-30T20:00:00Z", "THB", "10"),
            detail("end", "T0006", "2026-10-31T16:59:59Z", "THB", "10"),
            detail("after", "T0006", "2026-10-31T17:00:00Z", "THB", "10"),
        ],
        &bounds,
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r.0.transaction.transaction_id.as_str())
            .collect::<Vec<_>>(),
        vec!["start", "october", "end"]
    );
}
#[test]
fn withdrawal_context_is_combined_before_filtering() {
    let rows = records(
        vec![
            detail("withdraw", "T0400", "2026-10-20T00:00:00Z", "THB", "0"),
            detail("thb", "T0201", "2026-10-19T00:00:00Z", "THB", "3600"),
            detail("usd", "T0201", "2026-10-18T00:00:00Z", "USD", "-100"),
            detail("sale", "T0006", "2026-09-15T00:00:00Z", "USD", "10"),
        ],
        &range("2026-10-01T00:00:00+07:00", "2026-10-31T23:59:59+07:00"),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, 36.0);
    assert_eq!(rows[0].2.to_rfc3339(), "2026-10-20T00:00:00+00:00");
}
#[test]
fn year_crossing_and_multi_month_ranges_keep_exact_bounds() {
    let rows = records(
        vec![
            detail("old", "T0006", "2025-12-01T00:00:00Z", "THB", "10"),
            detail("start", "T0006", "2026-11-30T22:00:00Z", "THB", "10"),
            detail("middle", "T0006", "2027-01-01T00:00:00Z", "THB", "10"),
            detail("end", "T0006", "2027-02-01T02:00:00+02:00", "THB", "10"),
        ],
        &range("2026-12-01T00:00:00+02:00", "2027-02-01T00:00:00Z"),
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
}
#[test]
fn overlap_identity_preserves_distinct_events_with_the_same_id() {
    let first = detail("same", "T0000", "2026-10-01T00:00:00.123Z", "THB", "10");
    let second = detail("same", "T0011", "2026-10-01T00:00:00.123Z", "THB", "10");
    assert_ne!(identity(&first), identity(&second));
    assert_eq!(
        identity(&first),
        identity(&detail(
            "same",
            "T0000",
            "2026-10-01T00:00:00.123Z",
            "THB",
            "10"
        ))
    );
}
