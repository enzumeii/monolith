#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn row(quantity: i64, price: &str) -> ProductReportRow {
    use crate::models::shopify::{Measurement, Weight};
    ReportRow::new(
        "Item".into(),
        None,
        Some(serde_json::from_value(json!("GENSTSWOW01")).unwrap()),
        Some("KEYCHAIN".into()),
        Money {
            amount: price.into(),
            currency_code: "USD".into(),
        },
        quantity,
        Some(InventoryMetadata {
            unit_cost: None,
            measurement: Measurement {
                weight: Some(Weight {
                    value: "2.5".parse().unwrap(),
                    unit: "GRAMS".into(),
                }),
            },
        }),
    )
    .unwrap()
    .into_dto()
}

#[test]
fn totals_use_exact_decimal_and_preserve_negative_inventory() {
    let mut value = row(3, "0.1");
    totals(&mut value).unwrap();
    assert_eq!(value.total_selling_price.to_string(), "0.3");
    assert_eq!(value.total_weight.as_ref().unwrap().to_string(), "7.5");
    value.quantity = -2;
    totals(&mut value).unwrap();
    assert_eq!(value.total_selling_price.to_string(), "-0.2");
    value.weight = None;
    totals(&mut value).unwrap();
    assert!(value.total_weight.is_none());
}

#[test]
fn scientific_weights_and_exact_scaled_totals_are_preserved() {
    for (raw, expected) in [
        ("1e-7", "0.0000001"),
        ("1e7", "10000000"),
        ("1.25E+2", "125"),
    ] {
        let metadata: InventoryMetadata = serde_json::from_str(&format!(
            r#"{{"unitCost":null,"measurement":{{"weight":{{"value":{raw},"unit":"GRAMS"}}}}}}"#
        ))
        .unwrap();
        let mut value = ReportRow::new(
            "Item".into(),
            None,
            Some(serde_json::from_value(json!("GENSTSWOW01")).unwrap()),
            Some("KEYCHAIN".into()),
            Money {
                amount: "1".into(),
                currency_code: "USD".into(),
            },
            3,
            Some(metadata),
        )
        .unwrap()
        .into_dto();
        assert_eq!(value.weight.as_ref().unwrap().to_string(), expected);
        totals(&mut value).unwrap();
    }
    let mut value = row(10, "7922816251426433759354395033.5");
    value.weight = None;
    totals(&mut value).unwrap();
    assert_eq!(
        value.total_selling_price.to_string(),
        "79228162514264337593543950335"
    );
    assert!(number("1e-29").is_err());
    let mut value = row(4_611_686_018_427_387_904, "1");
    value.weight = None;
    assert!(number("0.00000000116415321826934814453125").is_err());
    value.selling_price = number("0.0000000037252902984619140625").unwrap();
    totals(&mut value).unwrap();
    assert_eq!(value.total_selling_price.to_string(), "17179869184");
}

#[test]
fn numbers_and_quantity_overflow_fail_without_rounding() {
    assert!(number("NaN").is_err());
    assert!(number("0.00000000000000000000000000001").is_err());
    let mut value = ReportRow(row(1, "1"));
    value.0.quantity = i64::MAX;
    assert!(value.add_quantity(1).is_err());
    let mut value = row(1, "79228162514264337593543950335");
    value.quantity = i64::MAX;
    assert!(totals(&mut value).is_err());
    let value = row(30, "1234567890123456789012345678.1");
    assert_eq!(
        value.total_selling_price.to_string(),
        "37037036703703703670370370343"
    );
}
#[test]
fn compatible_rows_compare_normalized_numbers_and_metadata() {
    let mut first = ReportRow(row(2, "1.00"));
    let second = ReportRow(row(3, "1"));
    assert!(first.can_merge(&second));
    first.add_quantity(second.quantity()).unwrap();
    assert_eq!(first.0.quantity, 5);
    assert_eq!(first.0.total_selling_price.to_string(), "5");
    for field in ["currency", "cost", "unit", "sku", "missing"] {
        let mut other = row(1, "1");
        match field {
            "currency" => other.selling_price_currency = "THB".into(),
            "cost" => other.cost = Some(1.into()),
            "unit" => other.unit = Some("POUNDS".into()),
            "sku" => other.sku = Some("OTHER".into()),
            _ => other.sku = None,
        }
        assert!(!first.can_merge(&ReportRow(other)));
    }
}
#[test]
fn invalid_report_values_keep_the_model_origin() {
    let error = number("invalid-price").unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Protocol(_)));
    assert_eq!(
        error.location().file(),
        "services/api/src/models/products.rs"
    );
    assert!(error.location().line() > 0);
}
