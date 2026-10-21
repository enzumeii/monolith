use super::*;
use crate::export::tests::xml;

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
