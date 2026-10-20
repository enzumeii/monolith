use super::{Sku, SkuCategory};
use serde_json::json;

#[test]
fn sku_wire_value_survives_shared_classification() {
    for (raw, expected) in [
        ("กขคAPP001", SkuCategory::Apparel),
        ("ABCBLK_SS", SkuCategory::Stock),
        ("ABCUNK001", SkuCategory::Unknown),
    ] {
        let sku: Sku = serde_json::from_value(json!(raw)).expect("SKU string");
        assert_eq!(SkuCategory::from_sku(raw), expected);
        assert_eq!(serde_json::to_value(&sku).expect("SKU string"), json!(raw));
        assert_eq!(sku.into_string(), raw);
    }
}

#[test]
fn report_sku_category_uses_character_positions() {
    assert_eq!(SkuCategory::from_sku("กขคAPP001").as_str(), "APPAREL");
    assert_eq!(SkuCategory::from_sku("กขคKEY001").as_str(), "KEYCHAIN");
    assert_eq!(SkuCategory::from_sku("กขคAPP_SS").as_str(), "STOCK");
    assert_eq!(SkuCategory::from_sku("短").as_str(), "");
}

#[test]
fn report_category_labels_keep_each_product_group() {
    for (code, expected) in [
        ("APP", "APPAREL"),
        ("BBT", "APPAREL"),
        ("TEE", "APPAREL"),
        ("TOT", "BAG"),
        ("BLK", "BLANKET"),
        ("QCS", "PHONE_ACCESSORY"),
        ("CSE", "PHONE_ACCESSORY"),
        ("GTK", "PHONE_ACCESSORY"),
        ("PSR", "PHONE_ACCESSORY"),
        ("CLC", "CLAWCLIP"),
        ("PIN", "BADGE"),
        ("KEY", "KEYCHAIN"),
        ("QSK", "KEYCHAIN"),
        ("LNK", "KEYCHAIN"),
        ("WDK", "KEYCHAIN"),
        ("KMB", "KEYCHAIN"),
        ("LQD", "KEYCHAIN"),
        ("CRB", "CARABINER"),
        ("PSH", "PLUSHIE"),
        ("STD", "STANDEE"),
        ("STS", "STICKER_SHEET"),
        ("PTC", "STICKER_SHEET"),
        ("PCT", "PHOTOCARD"),
        ("MAG", "MAGNET"),
        ("STK", "STICKER"),
        ("BST", "STICKER"),
        ("ZPB", "ZIPPER_BAG"),
        ("BDS", "BEAD"),
        ("CLP", "HAIRCLIP"),
        ("LYD", "LANYARD"),
        ("CHD", "CARDHOLDER"),
        ("UNK", ""),
    ] {
        assert_eq!(
            SkuCategory::from_sku(&format!("ABC{code}001")).as_str(),
            expected
        );
    }
    for marker in ["_IP", "_SS", "_XM"] {
        assert_eq!(
            SkuCategory::from_sku(&format!("ABCBLK{marker}001")).as_str(),
            "STOCK"
        );
    }
}
