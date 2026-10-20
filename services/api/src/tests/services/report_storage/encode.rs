#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;
use std::io::{Cursor, Read};

#[test]
fn inventory_csv_preserves_numbers_quotes_and_protects_formula_text() {
    let files = encode_value("INVENTORY", &json!({"inventory":[{
        "sku":"=HYPERLINK(\"https://example.test\")", "title":"Hammer, large", "selling_price":-4.5,"quantity":2
    }]}), ReportFileFormat::Csv).unwrap();
    assert_eq!(files[0].filename, "inventory.csv");
    let csv = String::from_utf8(files[0].bytes.clone()).unwrap();
    assert!(csv.starts_with("category,collection,sku,title,"));
    assert!(csv.contains("'=HYPERLINK(\"\"https://example.test\"\""));
    assert!(csv.contains("\"Hammer, large\""));
    assert!(csv.ends_with(",,-4.5,,2,,\r\n"));
}

#[test]
fn accounting_csv_retains_both_providers_currency_and_items() {
    let files = encode_value("TRANSACTION", &json!({
        "transactions":{"stripe":[{"gateway":"STRIPE","currency":"THB","payment_reference":"s"}],
                        "paypal":[{"gateway":"PAYPAL","currency":"USD","payment_reference":"p"}]},
        "items":[{"sku":"item","quantity":2}]
    }), ReportFileFormat::Csv).unwrap();
    assert_eq!(
        files
            .iter()
            .map(|file| file.filename.as_str())
            .collect::<Vec<_>>(),
        ["transactions.csv", "items.csv"]
    );
    let csv = String::from_utf8(files[0].bytes.clone()).unwrap();
    assert!(csv.contains("customer_phone,currency,exchange_rate"));
    assert!(csv.contains("STRIPE"));
    assert!(csv.contains("PAYPAL"));
    assert!(csv.contains("THB"));
    assert!(
        String::from_utf8(files[1].bytes.clone())
            .unwrap()
            .contains("item")
    );
}

#[test]
fn xlsx_has_both_named_sheets_and_writes_strings_without_formulas() {
    let files = encode_value("TRANSACTION", &json!({
        "transactions":{"stripe":[{"description":"=1+1","shopify_order_id":18_446_744_073_709_551_615_u64}],"paypal":[]},
        "items":[]
    }), ReportFileFormat::Xlsx).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename, "transactions.xlsx");
    let mut archive = zip::ZipArchive::new(Cursor::new(&files[0].bytes)).unwrap();
    let mut workbook = String::new();
    archive
        .by_name("xl/workbook.xml")
        .unwrap()
        .read_to_string(&mut workbook)
        .unwrap();
    assert!(workbook.contains("Transactions"));
    assert!(workbook.contains("Items"));
    let mut strings = String::new();
    archive
        .by_name("xl/sharedStrings.xml")
        .unwrap()
        .read_to_string(&mut strings)
        .unwrap();
    assert!(strings.contains("=1+1"));
    assert!(strings.contains("18446744073709551615"));
    let mut sheet = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut sheet)
        .unwrap();
    assert!(!sheet.contains("<f>"));
}

#[test]
fn empty_inventory_still_has_headers_and_invalid_saved_shapes_fail() {
    let files = encode_value("INVENTORY", &json!({"inventory":[]}), ReportFileFormat::Csv).unwrap();
    assert_eq!(
        String::from_utf8(files[0].bytes.clone())
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(
        encode_value(
            "TRANSACTION",
            &json!({"transactions":{}}),
            ReportFileFormat::Csv
        )
        .is_err()
    );
}
