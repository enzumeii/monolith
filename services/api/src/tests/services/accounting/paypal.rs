#![allow(clippy::unwrap_used, clippy::float_cmp)]
use super::{CurrencyCode, identity, precision_factor, records};
use crate::{models::accounting::AccountingDateRange, models::paypal::TransactionDetail};
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
            detail("manual", "T0403", "2026-10-20T00:00:00Z", "THB", "3600"),
            detail("conversion", "T0200", "2026-10-19T00:00:00Z", "THB", "3600"),
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

#[test]
fn unknown_paypal_events_keep_distinct_identities_and_sale_behavior() {
    let first = detail("same", "T9999", "2026-10-01T00:00:00Z", "AUD", "10");
    let second = detail("same", "T9998", "2026-10-01T00:00:00Z", "AUD", "10");
    assert_ne!(identity(&first), identity(&second));
    let row = super::payment(&first).unwrap().transaction;
    assert_eq!(row.kind, domain::dto::AccountingKind::Sale);
    assert_eq!(row.gross_paid, 10.0);
}

#[test]
fn paypal_event_mappings_keep_descriptions_and_refund_classification() {
    for (event, expected) in [
        (
            "T2105",
            "Payment Held for fraudulent activity in currency AUD",
        ),
        (
            "T2106",
            "Payment Released from fraud review in currency AUD",
        ),
        (
            "T1110",
            "Hold the amount for chargeback review from customer Dispute in currency AUD",
        ),
        (
            "T1111",
            "Cancel the charge back and release the amount back to us in currency AUD",
        ),
        (
            "T1106",
            "Refund the amount to customer due to dispute lost in currency AUD",
        ),
        ("T0114", "Dispute Fee in currency AUD"),
        ("T0000", "PAID TO  in currency AUD"),
        ("T0011", "PAID TO  in currency AUD"),
        ("T0006", ""),
        ("T1107", ""),
    ] {
        let row = super::payment(&detail("event", event, "2026-10-01T00:00:00Z", "AUD", "10"))
            .unwrap()
            .transaction;
        assert_eq!(row.description, expected);
        assert_eq!(
            row.kind,
            if event == "T1107" {
                domain::dto::AccountingKind::Refund
            } else {
                domain::dto::AccountingKind::Sale
            }
        );
    }
}

#[test]
fn all_supported_paypal_currencies_keep_their_native_precision() {
    for (codes, expected) in [
        (&["HUF", "JPY", "TWD"][..], 10.0),
        (
            &["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"][..],
            10.124,
        ),
        (
            &[
                "AUD", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "ILS", "MYR",
                "MXN", "NOK", "NZD", "PHP", "PLN", "SEK", "SGD", "THB", "USD",
            ][..],
            10.12,
        ),
    ] {
        for currency in codes {
            let row = super::payment(&detail(
                "precision",
                "T0006",
                "2026-10-01T00:00:00Z",
                currency,
                "10.1235",
            ))
            .unwrap()
            .transaction;
            assert_eq!(row.gross_paid, expected, "{currency}");
            assert_eq!(row.currency, *currency);
        }
    }
}

#[test]
fn newly_named_convention_currencies_remain_unsupported_for_accounting() {
    for code in [
        "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AWG", "AZN", "BAM", "BBD", "BDT", "BGN",
        "BIF", "BMD", "BND", "BOB", "BSD", "BWP", "BYN", "BZD", "CDF", "CLP", "COP", "CRC", "CVE",
        "DJF", "DOP", "DZD", "EGP", "ETB", "FJD", "FKP", "GEL", "GIP", "GMD", "GNF", "GTQ", "GYD",
        "HNL", "HTG", "IDR", "INR", "ISK", "JMD", "KES", "KGS", "KHR", "KMF", "KRW", "KYD", "KZT",
        "LAK", "LBP", "LKR", "LRD", "LSL", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP", "MUR",
        "MVR", "MWK", "MZN", "NAD", "NGN", "NIO", "NPR", "PAB", "PEN", "PGK", "PKR", "PYG", "QAR",
        "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SHP", "SLE", "SOS", "SRD", "STD", "SZL",
        "TJS", "TOP", "TRY", "TTD", "TZS", "UAH", "UGX", "UYU", "UZS", "VND", "VUV", "WST", "XAF",
        "XCD", "XOF", "XPF", "YER", "ZAR", "ZMW",
    ] {
        assert!(
            precision_factor(&CurrencyCode::from(code.to_owned())).is_err(),
            "{code}"
        );
    }
}

#[test]
fn only_successful_paypal_sale_events_have_external_source_money() {
    for (event, status, eligible) in [
        ("T0006", "S", true),
        ("T0000", "S", true),
        ("T0011", "S", true),
        ("T0006", "P", false),
        ("T0006", "D", false),
        ("T1107", "S", false),
        ("T9999", "S", false),
        ("T2105", "S", false),
    ] {
        let mut value = json!({"transaction_info":{"transaction_id":"capture-source", "transaction_event_code":event,
            "transaction_initiation_date":"2026-10-01T00:00:00Z", "transaction_updated_date":"2026-10-01T00:00:00Z",
            "transaction_status":status, "transaction_amount":{"currency_code":"AUD","value":"4.00"}}});
        let row = super::payment(&serde_json::from_value(value.clone()).unwrap()).unwrap();
        assert_eq!(row.source_money.is_some(), eligible, "{event}/{status}");
        if let Some(source) = row.source_money {
            assert_eq!(source.minor, 400);
            assert_eq!(source.currency, "AUD");
        }
        value["transaction_info"]["transaction_amount"]["value"] = json!("0.00");
        let row = super::payment(&serde_json::from_value(value).unwrap()).unwrap();
        assert!(row.source_money.is_none());
    }
}
