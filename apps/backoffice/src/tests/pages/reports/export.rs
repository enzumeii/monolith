use super::*;
use std::io::{Cursor, Read};
fn row() -> ProductReportRow {
    ProductReportRow {
        category: None,
        collection: Some("é".into()),
        sku: Some("00123".into()),
        title: "=1+1\nA \"quote\", é".into(),
        variant: None,
        cost: Some("0.00".parse().expect("decimal")),
        cost_currency: None,
        weight: None,
        unit: None,
        selling_price: "1.2300".parse().expect("decimal"),
        selling_price_currency: "THB".into(),
        quantity: 0,
        total_selling_price: "0.00".parse().expect("decimal"),
        total_weight: None,
    }
}
fn xml(bytes: &[u8], path: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid XLSX ZIP archive");
    let mut text = String::new();
    zip.by_name(path)
        .expect("workbook entry")
        .read_to_string(&mut text)
        .expect("XML text");
    text
}
#[test]
fn filename_changes_keep_extension_and_format_consistent() {
    assert_eq!(
        ExportFormat::Csv
            .filename("convention-report.csv")
            .expect("name"),
        "convention-report.csv"
    );
    assert_eq!(
        ExportFormat::Xlsx
            .filename("  renamed.csv  ")
            .expect("name"),
        "renamed.xlsx"
    );
    assert_eq!(
        ExportFormat::Csv.filename("picklist.XLSX").expect("name"),
        "picklist.csv"
    );
    assert_eq!(
        ExportFormat::Xlsx.filename("renamed").expect("name"),
        "renamed.xlsx"
    );
    assert_eq!(
        ExportFormat::from_filename("Report.XLSX").expect("type"),
        ExportFormat::Xlsx
    );
    assert_eq!(
        ExportFormat::from_filename("report.csv").expect("type"),
        ExportFormat::Csv
    );
    for name in [
        "",
        "  ",
        ".xlsx",
        "../report.csv",
        "folder/report.csv",
        "report.pdf",
        "report.csv.exe",
        "a\0.csv",
        r"folder\report.csv",
    ] {
        assert!(ExportFormat::Csv.filename(name).is_err(), "{name:?}");
        assert!(ExportFormat::from_filename(name).is_err(), "{name:?}");
    }
}
#[test]
fn xlsx_preserves_values_as_text_without_formulas() {
    let bytes = encode_report(&[row()], ExportFormat::Xlsx).expect("workbook");
    let strings = xml(&bytes, "xl/sharedStrings.xml");
    for value in [
        "00123",
        "1.2300",
        "0.00",
        "=1+1",
        "é",
        "category",
        "total_weight",
    ] {
        assert!(strings.contains(value), "missing {value}");
    }
    let sheet = xml(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("r=\"A1\""));
    assert!(sheet.contains("r=\"N1\""));
    assert!(sheet.contains("r=\"L2\" t=\"s\""));
    assert!(!sheet.contains("<f>"));
    assert!(xml(&bytes, "xl/workbook.xml").contains("name=\"Reports\""));
}
#[test]
fn empty_workbook_has_only_headers() {
    let bytes = encode_report(&[], ExportFormat::Xlsx).expect("empty workbook");
    let sheet = xml(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("A1:N1"));
    assert!(!sheet.contains("r=\"2\""));
}
#[test]
fn csv_encoder_keeps_existing_escaping_and_exact_values() {
    let bytes = encode_report(&[row()], ExportFormat::Csv).expect("CSV");
    let text = String::from_utf8(bytes).expect("UTF-8");
    assert!(text.contains(",é,00123,\"=1+1\nA \"\"quote\"\", é\""));
    assert!(text.ends_with(",1.2300,THB,0,0.00,\r\n"));
}
#[test]
fn xlsx_limits_are_errors_instead_of_truncation() {
    assert!(validate_row_count(1_048_575).is_ok());
    assert!(validate_row_count(1_048_576).is_err());
    let mut row = row();
    row.title = "a".repeat(32_768);
    assert!(encode_report(&[row], ExportFormat::Xlsx).is_err());
}
