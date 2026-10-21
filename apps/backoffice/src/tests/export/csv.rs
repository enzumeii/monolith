use super::*;
#[test]
fn generic_table_csv_escapes_cells_and_keeps_header_order() {
    assert_eq!(
        encode_table(
            &["Order", "Customer"],
            &[vec!["#123".into(), "A, \"B\"".into()]]
        ),
        "Order,Customer\r\n#123,\"A, \"\"B\"\"\"\r\n"
    );
}
