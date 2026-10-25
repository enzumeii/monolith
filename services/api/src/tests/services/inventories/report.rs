#![allow(clippy::unwrap_used)]
use super::*;
use crate::models::shopify::{InventoryMetadata, Money};
use serde_json::json;

fn row(quantity: i64, price: &str) -> ProductReportRow {
    use crate::models::shopify::{Measurement, Weight};
    ReportRow::new(ReportInput {
        title: "Item".into(),
        variant: None,
        sku: Some(serde_json::from_value(json!("GENSTSWOW01")).unwrap()),
        category: Some("KEYCHAIN".into()),
        price: Money {
            amount: price.into(),
            currency_code: "USD".into(),
        },
        quantity,
        metadata: Some(InventoryMetadata {
            unit_cost: None,
            measurement: Measurement {
                weight: Some(Weight {
                    value: "2.5".parse().unwrap(),
                    unit: "GRAMS".into(),
                }),
            },
        }),
    })
    .unwrap()
    .into_dto()
}

#[test]
fn totals_use_two_decimal_places_and_preserve_negative_inventory() {
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
fn scientific_weights_are_preserved_in_report_rows() {
    for (raw, expected) in [
        ("1e-7", "0.0000001"),
        ("1e7", "10000000"),
        ("1.25E+2", "125"),
    ] {
        let metadata: InventoryMetadata = serde_json::from_str(&format!(
            r#"{{"unitCost":null,"measurement":{{"weight":{{"value":{raw},"unit":"GRAMS"}}}}}}"#
        ))
        .unwrap();
        let mut value = ReportRow::new(ReportInput {
            title: "Item".into(),
            variant: None,
            sku: Some(serde_json::from_value(json!("GENSTSWOW01")).unwrap()),
            category: Some("KEYCHAIN".into()),
            price: Money {
                amount: "1".into(),
                currency_code: "USD".into(),
            },
            quantity: 3,
            metadata: Some(metadata),
        })
        .unwrap()
        .into_dto();
        assert_eq!(value.weight.as_ref().unwrap().to_string(), expected);
        totals(&mut value).unwrap();
    }
}

#[test]
fn quantity_overflow_keeps_the_service_origin() {
    let mut value = ReportRow(row(1, "1"));
    value.0.quantity = i64::MAX;
    let error = value.add_quantity(1).unwrap_err();
    assert!(matches!(error.kind(), ClientErrorKind::Protocol(_)));
    assert_eq!(
        error.location().file(),
        "services/api/src/services/inventories/report.rs"
    );
    assert!(error.location().line() > 0);
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
fn source_models_derive_collection_and_inventory_rows() {
    for (raw, want) in [
        ("GENSTSWOW01", Some("STSWOW")),
        ("กขคSTSWOW01", Some("STSWOW")),
        ("SHORT", None),
    ] {
        let sku: crate::models::Sku = serde_json::from_value(json!(raw)).unwrap();
        assert_eq!(sku.collection().as_deref(), want);
    }
    for (active, quantity) in [(true, 0), (true, -2), (false, 3)] {
        let value = json!({"id":"gid://shopify/ProductVariant/1","title":"Large","sku":"GENSTSWOW01","contextualPricing":{"price":{"amount":"0.1","currencyCode":"USD"}},"image":null,
            "inventoryItem":{"id":"gid://shopify/InventoryItem/1","tracked":true,"unitCost":null,"measurement":{"weight":null},"inventoryLevel":{"id":"level","isActive":active,"quantities":[{"name":"on_hand","quantity":quantity}]}}});
        let row = inventory_row(
            serde_json::from_value::<crate::models::shopify::Variant>(value).unwrap(),
            "Parent",
            "KEYCHAIN",
        )
        .unwrap();
        if active {
            let row = row.unwrap().into_dto();
            assert_eq!(row.quantity, quantity);
            assert_eq!(row.collection.as_deref(), Some("STSWOW"));
            assert_eq!(row.title, "Parent");
        } else {
            assert!(row.is_none());
        }
    }
}
#[test]
fn line_conversion_keeps_historical_values_and_missing_metadata() {
    let line:crate::models::shopify::TaggedOrderLineItem=serde_json::from_value(json!({"title":"Deleted","variantTitle":null,"sku":null,"quantity":3,"originalUnitPriceSet":{"shopMoney":{"amount":"0.1","currencyCode":"THB"}},"product":null,"variant":null})).unwrap();
    let row = ReportRow::new(crate::transformers::orders::report_input(line))
        .unwrap()
        .into_dto();
    assert_eq!(row.total_selling_price.to_string(), "0.3");
    assert!(row.cost.is_none());
    assert!(row.collection.is_none());
    assert_eq!(row.selling_price_currency, "THB");
}
#[test]
fn inventory_report_decodes_without_unused_catalog_fields() {
    let product: crate::models::shopify::Product = serde_json::from_value(json!({
        "title":"Parent", "productType":"KEYCHAIN",
        "variants":{"nodes":[{"title":"Large","sku":"GENSTSWOW01",
            "contextualPricing":{"price":{"amount":"2.5","currencyCode":"THB"}},
            "inventoryItem":{"unitCost":null,"measurement":{"weight":null},
                "inventoryLevel":{"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":2}]}}
        }],"pageInfo":{"hasNextPage":false,"endCursor":null}}
    })).unwrap();
    let row = inventory_row(
        product.variants.nodes.into_iter().next().unwrap(),
        &product.title,
        &product.product_type,
    )
    .unwrap()
    .unwrap()
    .into_dto();
    assert_eq!(row.quantity, 2);
    assert_eq!(row.total_selling_price.to_string(), "5");
}

#[test]
fn order_report_decodes_without_unused_order_header() {
    let order: crate::models::shopify::TaggedOrder = serde_json::from_value(json!({
        "lineItems":{"nodes":[{"title":"Item","variantTitle":null,"sku":null,"quantity":2,
            "originalUnitPriceSet":{"shopMoney":{"amount":"2.5","currencyCode":"THB"}},"product":null,"variant":null
        }],"pageInfo":{"hasNextPage":false,"endCursor":null}}
    })).unwrap();
    let row = ReportRow::new(crate::transformers::orders::report_input(
        order.line_items.nodes.into_iter().next().unwrap(),
    ))
    .unwrap()
    .into_dto();
    assert_eq!(row.quantity, 2);
    assert_eq!(row.total_selling_price.to_string(), "5");
}
