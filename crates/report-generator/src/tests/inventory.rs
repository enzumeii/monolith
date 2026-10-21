use super::*;

fn row() -> ProductReportRow {
    ProductReportRow {
        category: None,
        collection: Some("é".into()),
        sku: Some("a,b".into()),
        title: "A \"quote\"\nline".into(),
        variant: Some("<script>alert(1)</script>".into()),
        cost: Some("0.00".parse().expect("valid test fixture")),
        cost_currency: Some("THB".into()),
        weight: None,
        unit: None,
        selling_price: "1.2300".parse().expect("valid test fixture"),
        selling_price_currency: "THB".into(),
        quantity: 0,
        total_selling_price: "0.00".parse().expect("valid test fixture"),
        total_weight: None,
    }
}
#[test]
fn csv_preserves_decimal_null_zero_and_escapes_text() {
    let csv = encode(&[row()]);
    assert_eq!(
        csv,
        "category,collection,sku,title,variant,cost,cost_currency,weight,unit,selling_price,selling_price_currency,quantity,total_selling_price,total_weight\r\n,é,\"a,b\",\"A \"\"quote\"\"\nline\",<script>alert(1)</script>,0.00,THB,,,1.2300,THB,0,0.00,\r\n"
    );
}
#[test]
fn empty_csv_has_headers() {
    assert_eq!(encode(&[]), format!("{}\r\n", HEADERS.join(",")));
}

#[test]
fn inventory_numeric_policy_preserves_negative_values_and_guards_text() {
    let mut row = row();
    row.sku = Some("-1".into());
    row.title = "=cmd".into();
    row.variant = None;
    row.cost = Some("-2.50".parse().expect("valid test fixture"));
    row.weight = Some("-1.5".parse().expect("valid test fixture"));
    row.unit = Some("kg".into());
    row.selling_price = "-1.2300".parse().expect("valid test fixture");
    row.quantity = -2;
    row.total_selling_price = "-2.4600".parse().expect("valid test fixture");
    row.total_weight = Some("-3.0".parse().expect("valid test fixture"));
    assert_eq!(
        encode(&[row]),
        "category,collection,sku,title,variant,cost,cost_currency,weight,unit,selling_price,selling_price_currency,quantity,total_selling_price,total_weight\r\n,é,'-1,'=cmd,,-2.50,THB,-1.5,kg,-1.2300,THB,-2,-2.4600,-3.0\r\n"
    );
}
