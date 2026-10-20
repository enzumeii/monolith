use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Sku(String);
impl Sku {
    #[must_use]
    pub fn collection(&self) -> Option<String> {
        let value = self.0.chars().skip(3).take(6).collect::<String>();
        (value.chars().count() == 6).then_some(value)
    }
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkuCategory {
    Apparel,
    Bag,
    Blanket,
    PhoneAccessory,
    Clawclip,
    Badge,
    Keychain,
    Carabiner,
    Plushie,
    Standee,
    StickerSheet,
    Photocard,
    Magnet,
    Sticker,
    ZipperBag,
    Bead,
    Hairclip,
    Lanyard,
    Cardholder,
    Stock,
    Unknown,
}

impl SkuCategory {
    #[must_use]
    pub fn from_sku(sku: &str) -> Self {
        if ["_IP", "_SS", "_XM"]
            .iter()
            .any(|marker| sku.contains(marker))
        {
            return Self::Stock;
        }
        match sku.chars().skip(3).take(3).collect::<String>().as_str() {
            "APP" | "BBT" | "TEE" => Self::Apparel,
            "TOT" => Self::Bag,
            "BLK" => Self::Blanket,
            "QCS" | "CSE" | "GTK" | "PSR" => Self::PhoneAccessory,
            "CLC" => Self::Clawclip,
            "PIN" => Self::Badge,
            "KEY" | "QSK" | "LNK" | "WDK" | "KMB" | "LQD" => Self::Keychain,
            "CRB" => Self::Carabiner,
            "PSH" => Self::Plushie,
            "STD" => Self::Standee,
            "STS" | "PTC" => Self::StickerSheet,
            "PCT" => Self::Photocard,
            "MAG" => Self::Magnet,
            "STK" | "BST" => Self::Sticker,
            "ZPB" => Self::ZipperBag,
            "BDS" => Self::Bead,
            "CLP" => Self::Hairclip,
            "LYD" => Self::Lanyard,
            "CHD" => Self::Cardholder,
            _ => Self::Unknown,
        }
    }
}
