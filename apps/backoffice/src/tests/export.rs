use super::*;
use std::io::{Cursor, Read};
pub(crate) fn xml(bytes: &[u8], path: &str) -> String {
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
fn xlsx_limits_are_errors_instead_of_truncation() {
    assert!(validate_row_count(1_048_575).is_ok());
    assert!(validate_row_count(1_048_576).is_err());
}
#[test]
fn generic_workbook_exports_supplied_headers_and_rows_as_text() {
    let bytes =
        encode_table_xlsx(&["Order"], &[vec!["=1+1".into()]], "ThaiPost").expect("workbook");
    let strings = xml(&bytes, "xl/sharedStrings.xml");
    assert!(strings.contains("Order"));
    assert!(strings.contains("=1+1"));
    assert!(xml(&bytes, "xl/workbook.xml").contains("name=\"ThaiPost\""));
    assert!(!xml(&bytes, "xl/worksheets/sheet1.xml").contains("<f>"));
}
