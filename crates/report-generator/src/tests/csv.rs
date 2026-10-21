use super::*;

#[test]
fn quotes_text_and_prevents_formulas_without_changing_numeric_amounts() {
    assert_eq!(
        encode_table(&["title"], &[vec!["=cmd,\"quoted\"".into()]]),
        "title\r\n\"'=cmd,\"\"quoted\"\"\"\r\n"
    );
    let mut row = vec![String::new(); HEADERS.len()];
    row[2] = "-1".into();
    row[9] = "-1.25".into();
    let csv = encode_table(&HEADERS, &[row]);
    assert!(csv.contains("'-1"));
    assert!(csv.contains(",-1.25,"));
    assert!(!csv.contains("'-1.25"));
}
#[test]
fn empty_reports_keep_headers() {
    assert_eq!(encode(&[]), HEADERS.join(",") + "\r\n");
}
