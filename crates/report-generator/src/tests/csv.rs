use super::*;

#[test]
fn quotes_text_and_prevents_formulas_without_changing_explicit_numeric_columns() {
    assert_eq!(
        encode_table(
            &["arbitrary", "quantity"],
            &[vec!["-1.25".into(), "-1".into()]],
            &[0]
        ),
        "arbitrary,quantity\r\n-1.25,'-1\r\n"
    );
    assert_eq!(
        encode_table(&["text"], &[vec!["=cmd,\"quoted\"\nline".into()]], &[]),
        "text\r\n\"'=cmd,\"\"quoted\"\"\nline\"\r\n"
    );
}

#[test]
fn guards_formulas_and_nonfinite_or_malformed_numeric_cells() {
    for (value, expected) in [
        ("=1", "'=1"),
        ("+1", "'+1"),
        ("-1", "'-1"),
        ("@x", "'@x"),
        ("\tx", "'\tx"),
        ("\rx", "\"'\rx\""),
        ("  =1", "'  =1"),
        ("", ""),
    ] {
        assert_eq!(
            encode_table(&["x"], &[vec![value.into()]], &[]),
            format!("x\r\n{expected}\r\n")
        );
    }
    for (value, expected) in [
        ("-1.25", "-1.25"),
        ("+1", "+1"),
        ("-inf", "'-inf"),
        ("NaN", "NaN"),
        ("inf", "inf"),
        ("-bad", "'-bad"),
        ("=1", "'=1"),
        ("", ""),
    ] {
        assert_eq!(
            encode_table(&["anything"], &[vec![value.into()]], &[0]),
            format!("anything\r\n{expected}\r\n")
        );
    }
}
