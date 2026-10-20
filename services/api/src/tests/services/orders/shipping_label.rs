use super::common::*;
use super::*;
use crate::models::shopify::ShippingItem;
use serde_json::Number;
use serde_json::json;

#[test]
fn country_redirects_precede_method_aliases() {
    for (country, method, want) in [
        ("US", "No tracking", Courier::Fastship),
        ("TH", "USPS", Courier::Best),
        ("DE", "Tracking number", Courier::Fastship),
        ("DE", "Standard Shipping", Courier::ThaiPost),
        (
            "SG",
            "TELEPORT AIRASIA : TRACKING & FAST",
            Courier::Teleport,
        ),
        ("AU", "[NEW] Tax cleared shipping", Courier::Fastship),
        ("CA", "Best express", Courier::Best),
        ("SG", "Standard Shipping", Courier::ThaiPost),
        ("JP", "Tracking number", Courier::ThaiPost),
    ] {
        assert_eq!(
            select_courier(&normalize_method(method, country)),
            Some(want)
        );
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
    let (quantity, price, category) = declaration(items).expect("valid fixture");
    assert_eq!(quantity, 6);
    assert_eq!(price, Number::from_f64(600.0).expect("valid fixture"));
    assert_eq!(category, "Blankets");
    assert_eq!(
        declaration(vec![]).expect("valid fixture").1,
        Number::from_f64(100.0).expect("valid fixture")
    );
}

#[test]
fn best_labels_preserve_reference_zero_weight_without_requiring_order_weight() {
    let raw = json!({"id":"gid://shopify/Order/1","email":null,"totalWeight":null,"shippingAddress":{"firstName":"A","lastName":"B","name":"A B","address1":"1 Main","address2":null,"city":"Bangkok","provinceCode":null,"zip":"00123","country":"Thailand","countryCodeV2":"TH","phone":"123"},"shippingLines":{"nodes":[{"code":"Standard Shipping"}]},"lineItems":{"nodes":[]}});
    let parcel = Parcel::from_order(serde_json::from_value(raw.clone()).expect("valid fixture"))
        .expect("valid parcel");
    assert_eq!(parcel.email, "shipping@enzumeii.com");
    let order: ShippingOrder = serde_json::from_value(raw).expect("valid fixture");
    let mut labels = ShippingLabels::default();
    assert_eq!(
        append_label(order, None, &mut labels).expect("valid fixture"),
        Courier::Best
    );
    assert_eq!(labels.best[0].weight, 0);
}

#[test]
fn declaration_prices_follow_legacy_markup_for_any_currency() {
    for currency in ["THB", "USD", "EUR"] {
        let item = |title: &str, sku: &str, quantity, amount: &str| {
            serde_json::from_value::<ShippingItem>(json!({"title":title,"sku":sku,"quantity":quantity,"originalUnitPriceSet":{"shopMoney":{"amount":amount,"currencyCode":currency}}})).expect("selected price fields")
        };
        let (quantity, price, _) = declaration(vec![
            item("Keychain", "ABCKEY000001", 3, "30"),
            item("Stock", "ABCBLK000001_IP", 99, "10"),
            item("Tip", "", 1, "999"),
        ])
        .expect("currency-independent declaration");
        assert_eq!(quantity, 3);
        assert_eq!(price, Number::from_f64(200.0).expect("finite price"));
        let (_, low, _) =
            declaration(vec![item("Keychain", "ABCKEY000001", 7, "1")]).expect("minimum price");
        assert_eq!(low, Number::from_f64(100.0).expect("finite price"));
        for invalid in ["-1", "NaN", "inf", "1e309", "1e308"] {
            assert!(declaration(vec![item("Keychain", "ABCKEY000001", 1, invalid)]).is_err());
        }
    }
}
