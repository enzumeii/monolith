#![allow(clippy::unwrap_used)]
use crate::models::stripe::{BalanceTransactionKind, FeeKind};
use serde_json::json;

#[test]
fn stripe_kinds_keep_exact_known_and_unknown_wire_values() {
    for (wire, expected) in [
        ("charge", BalanceTransactionKind::Charge),
        ("payment", BalanceTransactionKind::Payment),
        ("refund", BalanceTransactionKind::Refund),
        ("payment_refund", BalanceTransactionKind::PaymentRefund),
        (
            "future_type",
            BalanceTransactionKind::Other("future_type".into()),
        ),
    ] {
        let kind: BalanceTransactionKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(kind, expected);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
    }
    for (wire, expected) in [
        ("stripe_fee", FeeKind::StripeFee),
        ("tax", FeeKind::Tax),
        ("future_fee", FeeKind::Other("future_fee".into())),
    ] {
        let kind: FeeKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(kind, expected);
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
    }
}
