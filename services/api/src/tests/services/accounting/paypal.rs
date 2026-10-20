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
fn transaction_bounds_compare_instants_including_offset_edges() {
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
    );
    assert_eq!(
        rows.iter()
            .map(|r| r.transaction.transaction_id.as_str())
            .collect::<Vec<_>>(),
        vec!["start", "october", "end"]
    );
}
#[test]
fn conversion_and_withdrawals_never_invent_a_payment_rate_or_payout() {
    let rows = records(
        vec![
            detail("withdraw", "T0400", "2026-10-20T00:00:00Z", "THB", "3600"),
            detail("thb", "T0201", "2026-10-19T00:00:00Z", "THB", "3600"),
            detail("usd", "T0201", "2026-10-18T00:00:00Z", "USD", "-100"),
            detail("old-sale", "T0006", "2026-09-15T00:00:00Z", "USD", "10"),
            detail("sale", "T0006", "2026-10-01T00:00:00Z", "SGD", "10"),
        ],
        &range("2026-10-01T00:00:00+07:00", "2026-10-31T23:59:59+07:00"),
    );
    assert_eq!(rows.len(), 1);
    let row = super::payment(&rows[0]).unwrap().transaction;
    assert_eq!(row.gross_paid, 10.0);
    assert_eq!(row.currency, "SGD");
    assert!(row.po_date.is_none());
    assert!(row.exchange_rate.is_none());
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
    );
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

#[test]
fn paypal_aud_sample_keeps_native_amounts_and_reported_shipping() {
    let mut value = serde_json::to_value(serde_json::json!({"transaction_info":{
        "transaction_id":"fixture-aud", "transaction_event_code":"T0006",
        "transaction_initiation_date":"2026-10-01T18:36:07Z", "transaction_updated_date":"2026-10-01T18:36:07Z",
        "transaction_status":"S", "transaction_amount":{"currency_code":"AUD","value":"53.08"},
        "fee_amount":{"currency_code":"AUD","value":"-2.82"},
        "shipping_amount":{"currency_code":"AUD","value":"10.08"}
    }})).unwrap();
    let detail: TransactionDetail = serde_json::from_value(value.take()).unwrap();
    let row = super::payment(&detail).unwrap().transaction;
    let row = serde_json::to_value(row).unwrap();
    assert_eq!(row["currency"], "AUD");
    assert_eq!(row["exchange_rate"], serde_json::Value::Null);
    assert_eq!(row["po_date"], serde_json::Value::Null);
    assert_eq!(row["gross_paid"], 53.08);
    assert_eq!(row["net_paid"], 50.26);
    assert_eq!(row["fee"], 2.82);
    assert_eq!(row["vat"], 0.0);
    assert_eq!(row["shipping_fee"], 10.08);
}

#[test]
fn paypal_fee_reversals_and_currency_precision_reconcile() {
    for (currency, gross, fee, expected_net, expected_fee) in [
        ("AUD", "-53.08", "2.82", -50.26, -2.82),
        ("AUD", "-53.08", "-2.82", -55.90, 2.82),
        ("JPY", "100", "-3", 97.0, 3.0),
        ("TND", "10.123", "-0.003", 10.120, 0.003),
    ] {
        let mut raw = json!({"transaction_info":{"transaction_id":"fixture", "transaction_event_code":"T1107", "transaction_initiation_date":"2026-10-01T00:00:00Z", "transaction_updated_date":"2026-10-01T00:00:00Z", "transaction_status":"S", "transaction_amount":{"currency_code":currency,"value":gross}, "fee_amount":{"currency_code":currency,"value":fee}}});
        let row = super::payment(&serde_json::from_value(raw.clone()).unwrap())
            .unwrap()
            .transaction;
        assert_eq!(row.net_paid, expected_net);
        assert_eq!(row.fee, expected_fee);
        assert!((row.gross_paid - row.net_paid - row.fee - row.vat).abs() < 0.000_001);
        assert_eq!(row.shipping_fee, None);
        raw["transaction_info"]["fee_amount"]["currency_code"] = json!("EUR");
        assert!(super::payment(&serde_json::from_value(raw).unwrap()).is_err());
    }
}

#[test]
fn paypal_rejects_incompatible_shipping_unknown_currency_and_overflow() {
    for (currency, amount, shipping) in [
        ("AUD", "10", Some("USD")),
        ("ZZZ", "10", None),
        ("AUD", "1e308", None),
    ] {
        let mut raw = json!({"transaction_info":{"transaction_id":"fixture", "transaction_event_code":"T0006", "transaction_initiation_date":"2026-10-01T00:00:00Z", "transaction_updated_date":"2026-10-01T00:00:00Z", "transaction_status":"S", "transaction_amount":{"currency_code":currency,"value":amount}}});
        if let Some(currency) = shipping {
            raw["transaction_info"]["shipping_amount"] =
                json!({"currency_code":currency,"value":"2"});
        }
        assert!(super::payment(&serde_json::from_value(raw).unwrap()).is_err());
    }
}

#[test]
fn paypal_zero_digit_currencies_round_each_reported_amount() {
    for currency in ["HUF", "TWD", "JPY"] {
        let raw = json!({"transaction_info":{"transaction_id":"zero-digit", "transaction_event_code":"T0006", "transaction_initiation_date":"2026-10-01T00:00:00Z", "transaction_updated_date":"2026-10-01T00:00:00Z", "transaction_status":"S", "transaction_amount":{"currency_code":currency,"value":"100.5"}, "fee_amount":{"currency_code":currency,"value":"-2.5"}, "shipping_amount":{"currency_code":currency,"value":"10.5"}}});
        let row = super::payment(&serde_json::from_value(raw).unwrap())
            .unwrap()
            .transaction;
        assert_eq!(row.gross_paid, 101.0);
        assert_eq!(row.fee, 3.0);
        assert_eq!(row.net_paid, 98.0);
        assert_eq!(row.shipping_fee, Some(11.0));
    }
}
