use super::OrderSummary;
use serde_json::json;

#[test]
fn listing_order_roundtrips_nullable_customer_contact_fields() {
    for value in [
        json!({"id":42,"number":1085,"full_name":"Jane Doe","email":"jane@example.test","shipping_country_code":"TH"}),
        json!({"id":42,"number":1085,"full_name":null,"email":null,"shipping_country_code":null}),
    ] {
        let order: OrderSummary = serde_json::from_value(value.clone()).expect("decode order");
        assert_eq!(serde_json::to_value(order).expect("serialize order"), value);
    }
}
