use super::*;
use serde_json::json;

#[test]
fn shipping_categories_preserve_declaration_labels_and_priorities() {
    for (code, expected) in [
        ("BLK", (8, "Blankets")),
        ("APP", (7, "Apparel")),
        ("BBT", (7, "Apparel")),
        ("TEE", (7, "Apparel")),
        ("PSH", (6, "Collectible Items")),
        ("TOT", (5, "Tote Bag")),
        ("QCS", (4, "Phone Accessories")),
        ("CSE", (4, "Phone Accessories")),
        ("GTK", (4, "Phone Accessories")),
        ("PSR", (4, "Phone Accessories")),
        ("CLC", (3, "Hair Accessories")),
        ("PIN", (2, "Badge")),
        ("KEY", (1, "Keychains")),
        ("QSK", (1, "Keychains")),
        ("LNK", (1, "Keychains")),
        ("WDK", (1, "Keychains")),
        ("KMB", (1, "Keychains")),
        ("LQD", (1, "Keychains")),
        ("STD", (1, "Keychains")),
        ("ZPB", (1, "Keychains")),
        ("BDS", (1, "Keychains")),
        ("CHD", (1, "Keychains")),
        ("CLP", (1, "Keychains")),
        ("LYD", (1, "Keychains")),
        ("CRB", (1, "Keychains")),
        ("STS", (0, "Stickers")),
        ("PTC", (0, "Stickers")),
        ("PCT", (0, "Stickers")),
        ("MAG", (0, "Stickers")),
        ("STK", (0, "Stickers")),
        ("BST", (0, "Stickers")),
        ("UNK", (0, "Stickers")),
    ] {
        assert_eq!(category_for_sku(&format!("ABC{code}001")), expected);
        assert_eq!(category_for_sku(&format!("กขค{code}001")), expected);
    }
    for sku in ["", "短", "ABCBLK_IP001", "ABCBLK_SS001", "ABCBLK_XM001"] {
        assert_eq!(category_for_sku(sku), (0, "Stickers"));
    }
}

#[test]
fn parcel_phone_keeps_leading_zeroes_and_street_address_format() {
    let raw = json!({"id":"gid://shopify/Order/1","email":null,"totalWeight":null,"shippingAddress":{"firstName":"A","lastName":"B","name":"A B","address1":"1 Main","address2":"Unit 2","city":"Bangkok","provinceCode":null,"zip":"00123","country":"Thailand","countryCodeV2":"TH","phone":"00 (081) ٢3-45"},"shippingLines":{"nodes":[{"code":"Standard Shipping"}]},"lineItems":{"nodes":[]}});
    let parcel =
        Parcel::from_order(serde_json::from_value(raw.clone()).expect("fixture")).expect("parcel");
    assert_eq!(parcel.address.phone.as_deref(), Some("00081345"));
    assert_eq!(joined_address(&parcel.address), "1 Main Unit 2");
    for phone in [json!(null), json!("---")] {
        let mut missing = raw.clone();
        missing["shippingAddress"]["phone"] = phone;
        assert!(Parcel::from_order(serde_json::from_value(missing).expect("fixture")).is_err());
    }
}
