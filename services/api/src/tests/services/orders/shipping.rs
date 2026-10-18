use super::*;
use serde_json::json;

#[test]
fn country_redirects_precede_method_aliases() {
    for (country, method, want) in [
        ("US", "No tracking", "FASTSHIP"),
        ("TH", "USPS", "BEST"),
        ("DE", "Tracking number", "FASTSHIP"),
        ("DE", "Standard Shipping", "THAI_POST"),
        ("SG", "TELEPORT AIRASIA : TRACKING & FAST", "TELEPORT"),
        ("AU", "[NEW] Tax cleared shipping", "FASTSHIP"),
        ("CA", "Best express", "BEST"),
        ("SG", "Standard Shipping", "THAI_POST"),
        ("JP", "Tracking number", "THAI_POST"),
    ] {
        assert_eq!(select_courier(&normalize_method(method, country)), want);
    }
}
#[test]
fn sku_category_priority_and_declaration_preserve_legacy_quantity_rules() {
    let line = |sku: &str, title: &str, quantity| ShippingItem {
        title: title.into(),
        sku: Some(sku.into()),
        quantity,
        original_unit_price_set: serde_json::from_value(
            json!({"shopMoney":{"amount":"30","currencyCode":"THB"}}),
        )
        .expect("valid fixture"),
    };
    let items = vec![
        line("ABCKEY000001", "Keychain", 3),
        line("ABCBLK000001_IP", "Stock", 99),
        line("ABCPIN000001", "Badge", 2),
        line("ABCBLK000001", "Blanket", 1),
        line("", "Tip", 1),
    ];
    let (quantity, price, category) = declaration(items, "THB").expect("valid fixture");
    assert_eq!(quantity, 6);
    assert_eq!(price, Number::from_f64(600.0).expect("valid fixture"));
    assert_eq!(category, "Blankets");
    assert!(declaration(vec![], "USD").is_err());
    assert_eq!(
        declaration(vec![], "THB").expect("valid fixture").1,
        Number::from_f64(100.0).expect("valid fixture")
    );
}
#[test]
fn japanese_kanji_uses_japanese_readings_and_other_addresses_transliterate() {
    assert_eq!(romanize("東京都", "JP").to_lowercase(), "toukyouto");
    assert_eq!(romanize("L'été", "FR"), "Lete");
}
#[test]
fn best_labels_preserve_reference_zero_weight_without_requiring_order_weight() {
    let order:ShippingOrder=serde_json::from_value(json!({"id":"gid://shopify/Order/1","email":null,"currencyCode":"THB","totalWeight":null,"shippingAddress":{"firstName":"A","lastName":"B","name":"A B","address1":"1 Main","address2":null,"city":"Bangkok","provinceCode":null,"zip":"00123","country":"Thailand","countryCodeV2":"TH","phone":"123"},"shippingLines":{"nodes":[{"code":"Standard Shipping"}],"pageInfo":{"hasNextPage":false,"endCursor":null}},"lineItems":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}})).expect("valid fixture");
    let mut labels = ShippingLabels::default();
    assert_eq!(
        append_label(order, None, &mut labels).expect("valid fixture"),
        "BEST"
    );
    assert_eq!(labels.best[0].weight, 0);
}
