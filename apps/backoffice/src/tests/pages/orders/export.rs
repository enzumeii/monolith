use super::*;
use domain::dto::orders::{FastshipLabel, ThaiPostLabel};

fn fastship_label() -> FastshipLabel {
    FastshipLabel {
        firstname: "Alice".into(),
        lastname: "Smith".into(),
        email: "alice@example.test".into(),
        phone: "123".into(),
        address_1: "Street 1".into(),
        address_2: String::new(),
        city: "Bangkok".into(),
        state: String::new(),
        postcode: "10000".into(),
        country: "TH".into(),
        width: 1,
        height: 2,
        length: 3,
        weight: 4,
        category: "Goods".into(),
        quantity: 5,
        declared_value: 6_u64.into(),
        ioss: String::new(),
        note: String::new(),
        shipping_option: "Express".into(),
        reference_id: "order-7".into(),
        incoterm: "DDP".into(),
    }
}

fn thai_post_label() -> ThaiPostLabel {
    ThaiPostLabel {
        country_code: "US".into(),
        weight: 1,
        service_id: 2,
        category: "Goods".into(),
        sender_name: "Sender".into(),
        sender_phone: "123".into(),
        sender_email: "sender@example.test".into(),
        sender_address: "Sender address".into(),
        sender_sub_district: String::new(),
        sender_district: String::new(),
        sender_province: "Bangkok".into(),
        sender_postcode: "10000".into(),
        sender_country_code: "TH".into(),
        receiver_name: "Receiver".into(),
        receiver_phone: "456".into(),
        receiver_email: "receiver@example.test".into(),
        receiver_address: "Receiver address".into(),
        receiver_state: "CA".into(),
        receiver_city: "Los Angeles".into(),
        receiver_postcode: "90001".into(),
        currency: "USD".into(),
        item_description: "Sample".into(),
        quantity: 1,
        price: 20_u64.into(),
        origin: "TH".into(),
        ioss: String::new(),
        order_id: "1234".into(),
        insurance: "false".into(),
        hs_code: "0000".into(),
    }
}

#[test]
fn fastship_export_matches_provider_field_order() {
    let table = fastship_table(&[fastship_label()]);
    assert_eq!(table.headers[0], "firstname");
    assert_eq!(table.headers[9], "country");
    assert_eq!(table.headers[20], "reference_id");
    assert_eq!(table.rows[0][0], "Alice");
    assert_eq!(table.rows[0][9], "TH");
    assert_eq!(table.rows[0][20], "order-7");
    assert_eq!(table.headers.len(), table.rows[0].len());
}

#[test]
fn thai_post_export_matches_provider_field_order() {
    let table = thai_post_table(&[thai_post_label()]);
    assert_eq!(table.headers[0], "country_code");
    assert_eq!(table.headers[16], "receiver_address");
    assert_eq!(table.headers[27], "insurance");
    assert_eq!(table.rows[0][0], "US");
    assert_eq!(table.rows[0][16], "Receiver address");
    assert_eq!(table.rows[0][27], "false");
    assert_eq!(table.headers.len(), table.rows[0].len());
}
