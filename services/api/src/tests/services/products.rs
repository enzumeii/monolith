#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;

fn row(quantity: i64, price: &str) -> ProductReportRow {
    ProductReportRow {
        category: Some("KEYCHAIN".into()),
        collection: Some("STSWOW".into()),
        sku: Some("GENSTSWOW01".into()),
        title: "Item".into(),
        variant: None,
        cost: None,
        cost_currency: None,
        weight: Some(number("2.5").unwrap()),
        unit: Some("GRAMS".into()),
        selling_price: number(price).unwrap(),
        selling_price_currency: "USD".into(),
        quantity,
        total_selling_price: number("0").unwrap(),
        total_weight: None,
    }
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
fn merge_groups_sku_price_currency_metadata_and_recalculates_totals() {
    let mut rows = vec![];
    for value in [row(2, "1.00"), row(3, "1"), row(1, "2")] {
        merge(&mut rows, value).unwrap();
    }
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].quantity, 5);
    assert_eq!(rows[0].total_selling_price.to_string(), "5");
    let mut other = row(1, "1");
    other.selling_price_currency = "THB".into();
    merge(&mut rows, other).unwrap();
    let mut other = row(1, "1");
    other.unit = Some("POUNDS".into());
    merge(&mut rows, other).unwrap();
    for _ in 0..2 {
        let mut other = row(1, "1");
        other.sku = None;
        merge(&mut rows, other).unwrap();
    }
    assert_eq!(rows.len(), 6);
}

#[test]
fn overflow_and_invalid_upstream_numbers_are_rejected() {
    let mut value = row(i64::MAX, "79228162514264337593543950335");
    assert!(totals(&mut value).is_err());
    assert!(number("NaN").is_err());
    let mut value = row(3, "1234567890123456789012345678.1");
    assert!(totals(&mut value).is_ok());
    let mut value = row(30, "1234567890123456789012345678.1");
    totals(&mut value).unwrap();
    assert_eq!(
        value.total_selling_price.to_string(),
        "37037036703703703670370370343"
    );
    assert!(number("0.00000000000000000000000000001").is_err());
    let mut rows = vec![row(i64::MAX, "1")];
    assert!(merge(&mut rows, row(1, "1")).is_err());
}

#[test]
fn collection_uses_fourth_through_ninth_unicode_characters() {
    assert_eq!(collection(Some("GENSTSWOW01")), Some("STSWOW".into()));
    assert_eq!(collection(Some("กขคSTSWOW01")), Some("STSWOW".into()));
    assert_eq!(collection(Some("SHORT")), None);
    assert_eq!(collection(None), None);
    assert_eq!(
        serde_json::to_value(row(1, "0.1")).unwrap()["selling_price"],
        json!(0.1)
    );
}

#[test]
fn scientific_weights_and_exact_scaled_totals_are_preserved() {
    for (raw, expected) in [
        ("1e-7", "0.0000001"),
        ("1e7", "10000000"),
        ("1.25E+2", "125"),
    ] {
        let mut value = row(3, "1");
        let measurement: Measurement =
            serde_json::from_str(&format!(r#"{{"weight":{{"value":{raw},"unit":"GRAMS"}}}}"#))
                .unwrap();
        weight_metadata(&mut value, &measurement).unwrap();
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

#[tokio::test]
async fn unavailable_shopify_retains_constructor_cause() {
    use std::error::Error;
    let settings: Settings = serde_json::from_value(json!({
        "client":{"shopify":{"admin_api_url":"invalid-url","access_token":"fixture-token"}},
        "browser":{"public_origin":"http://localhost:8084","insecure_local":true},
        "server":{"host":"localhost","port":3000},
        "database":{"url":"unused","max_connections":1,"min_connections":1},
        "jwt":{"secret":"unused"}
    }))
    .unwrap();
    let error = product_report(&settings, ReportSource::OrderTag("test".into()))
        .await
        .unwrap_err();
    let mut cause = error.source();
    let mut found = false;
    while let Some(value) = cause {
        found |= value.to_string().contains("invalid Shopify URL");
        cause = value.source();
    }
    assert!(found);
    let response = axum::response::IntoResponse::into_response(error);
    assert_eq!(response.status(), 503);
}
