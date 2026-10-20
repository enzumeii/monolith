use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
#[allow(clippy::upper_case_acronyms)]
pub enum CurrencyCode {
    HUF,
    JPY,
    TWD,
    BHD,
    IQD,
    JOD,
    KWD,
    LYD,
    OMR,
    TND,
    AUD,
    BRL,
    CAD,
    CHF,
    CNY,
    CZK,
    DKK,
    EUR,
    GBP,
    HKD,
    ILS,
    MYR,
    MXN,
    NOK,
    NZD,
    PHP,
    PLN,
    SEK,
    SGD,
    THB,
    USD,
    Other(String),
}

impl CurrencyCode {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::HUF => "HUF",
            Self::JPY => "JPY",
            Self::TWD => "TWD",
            Self::BHD => "BHD",
            Self::IQD => "IQD",
            Self::JOD => "JOD",
            Self::KWD => "KWD",
            Self::LYD => "LYD",
            Self::OMR => "OMR",
            Self::TND => "TND",
            Self::AUD => "AUD",
            Self::BRL => "BRL",
            Self::CAD => "CAD",
            Self::CHF => "CHF",
            Self::CNY => "CNY",
            Self::CZK => "CZK",
            Self::DKK => "DKK",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::HKD => "HKD",
            Self::ILS => "ILS",
            Self::MYR => "MYR",
            Self::MXN => "MXN",
            Self::NOK => "NOK",
            Self::NZD => "NZD",
            Self::PHP => "PHP",
            Self::PLN => "PLN",
            Self::SEK => "SEK",
            Self::SGD => "SGD",
            Self::THB => "THB",
            Self::USD => "USD",
            Self::Other(value) => value,
        }
    }
}

impl From<String> for CurrencyCode {
    fn from(value: String) -> Self {
        let value = value.to_ascii_uppercase();
        match value.as_str() {
            "HUF" => Self::HUF,
            "JPY" => Self::JPY,
            "TWD" => Self::TWD,
            "BHD" => Self::BHD,
            "IQD" => Self::IQD,
            "JOD" => Self::JOD,
            "KWD" => Self::KWD,
            "LYD" => Self::LYD,
            "OMR" => Self::OMR,
            "TND" => Self::TND,
            "AUD" => Self::AUD,
            "BRL" => Self::BRL,
            "CAD" => Self::CAD,
            "CHF" => Self::CHF,
            "CNY" => Self::CNY,
            "CZK" => Self::CZK,
            "DKK" => Self::DKK,
            "EUR" => Self::EUR,
            "GBP" => Self::GBP,
            "HKD" => Self::HKD,
            "ILS" => Self::ILS,
            "MYR" => Self::MYR,
            "MXN" => Self::MXN,
            "NOK" => Self::NOK,
            "NZD" => Self::NZD,
            "PHP" => Self::PHP,
            "PLN" => Self::PLN,
            "SEK" => Self::SEK,
            "SGD" => Self::SGD,
            "THB" => Self::THB,
            "USD" => Self::USD,
            _ => Self::Other(value),
        }
    }
}

impl From<CurrencyCode> for String {
    fn from(value: CurrencyCode) -> Self {
        value.as_str().to_ascii_uppercase()
    }
}

#[cfg(test)]
#[path = "../tests/models/currency.rs"]
mod tests;
