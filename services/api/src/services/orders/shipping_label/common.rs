use crate::{
    client::{ClientError, ClientErrorKind},
    error::AppError,
    models::fulfillment::{ShippingAddress, ShippingItem, ShippingOrder},
};
use axum::http::StatusCode;
use domain::dto::orders::Courier;
use serde_json::Number;

pub(super) struct Parcel {
    pub(super) address: ShippingAddress,
    pub(super) country_code: String,
    pub(super) email: String,
    pub(super) weight: u64,
    pub(super) quantity: u64,
    pub(super) price: Number,
    pub(super) category: String,
    pub(super) method: String,
}

impl Parcel {
    pub(super) fn from_order(order: ShippingOrder) -> Result<Self, AppError> {
        let mut address = order
            .shipping_address
            .ok_or_else(|| invalid("Order shipping address missing"))?;
        let country_code = required(
            address.country_code_v2.as_deref(),
            "Order shipping country missing",
        )?
        .to_owned();
        for field in [
            &mut address.first_name,
            &mut address.last_name,
            &mut address.name,
            &mut address.address1,
            &mut address.address2,
            &mut address.city,
            &mut address.country,
        ] {
            *field = field.take().map(|value| romanize(&value, &country_code));
        }
        required(address.name.as_deref(), "Order recipient name missing")?;
        required(address.address1.as_deref(), "Order street address missing")?;
        required(address.city.as_deref(), "Order shipping city missing")?;
        required(address.country.as_deref(), "Order shipping country missing")?;
        address.phone = address
            .phone
            .map(|phone| phone.chars().filter(char::is_ascii_digit).collect());
        let lines = order
            .shipping_lines
            .into_complete_nodes()
            .map_err(|error| crate::services::common::upstream(error))?;
        let code = lines
            .first()
            .and_then(|line| line.code.as_deref())
            .filter(|code| !code.trim().is_empty())
            .ok_or_else(|| {
                AppError::new(
                    StatusCode::CONFLICT,
                    "Order has no supported shipping method",
                )
            })?;
        if !known_method(code) {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "Order has no supported shipping method",
            ));
        }
        let method = normalize_method(code, &country_code);
        if matches!(
            select_courier(&method),
            Some(Courier::Best | Courier::Teleport)
        ) {
            required(address.phone.as_deref(), "Order shipping phone missing")?;
            required(address.zip.as_deref(), "Order shipping postcode missing")?;
        }
        let (quantity, price, category) = declaration(
            order
                .line_items
                .into_complete_nodes()
                .map_err(|error| crate::services::common::upstream(error))?,
            &order.currency_code,
        )?;
        let weight = if select_courier(&method) == Some(Courier::Best) {
            0
        } else {
            order
                .total_weight
                .filter(|weight| *weight > 0)
                .ok_or_else(|| invalid("Order shipping weight missing or zero"))?
        };
        Ok(Self {
            address,
            country_code,
            email: order
                .email
                .filter(|email| !email.trim().is_empty())
                .unwrap_or_else(|| "shipping@enzumeii.com".into()),
            weight,
            quantity,
            price,
            category,
            method,
        })
    }
}

pub(super) fn declaration(
    items: Vec<ShippingItem>,
    currency: &str,
) -> Result<(u64, Number, String), AppError> {
    if currency != "THB" {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Shipping declarations require THB orders",
        ));
    }
    let mut quantity = 0u64;
    let mut price = 0.0;
    let mut category = (0, "Stickers");
    for item in items {
        let sku = item.sku.as_deref().unwrap_or_default();
        let (priority, name) = category_for_sku(sku);
        if priority > category.0 {
            category = (priority, name);
        }
        if item.title == "Tip" {
            continue;
        }
        if !["_IP", "_SS", "_XM"]
            .iter()
            .any(|marker| sku.contains(marker))
        {
            quantity = quantity
                .checked_add(item.quantity)
                .ok_or_else(|| invalid("Shipping quantity overflow"))?;
        }
        let money = item.original_unit_price_set.shop_money;
        if money.currency_code != "THB" {
            return Err(invalid("Shipping line currency differs from THB"));
        }
        let amount = money
            .amount
            .parse::<f64>()
            .ok()
            .filter(|price| price.is_finite() && *price >= 0.0)
            .ok_or_else(|| invalid("Invalid shipping declaration price"))?;
        price += amount;
    }
    let price = price * 5.0;
    if !price.is_finite() {
        return Err(invalid("Shipping declaration price overflow"));
    }
    let price = Number::from_f64(price.max(100.0))
        .ok_or_else(|| invalid("Invalid shipping declaration price"))?;
    Ok((quantity, price, category.1.into()))
}
fn category_for_sku(sku: &str) -> (u8, &'static str) {
    if ["_IP", "_SS", "_XM"]
        .iter()
        .any(|marker| sku.contains(marker))
    {
        return (0, "Stickers");
    }
    match sku.chars().skip(3).take(3).collect::<String>().as_str() {
        "BLK" => (8, "Blankets"),
        "APP" | "BBT" | "TEE" => (7, "Apparel"),
        "PSH" => (6, "Collectible Items"),
        "TOT" => (5, "Tote Bag"),
        "QCS" | "CSE" | "GTK" | "PSR" => (4, "Phone Accessories"),
        "CLC" => (3, "Hair Accessories"),
        "PIN" => (2, "Badge"),
        "KEY" | "QSK" | "LNK" | "WDK" | "KMB" | "LQD" | "STD" | "ZPB" | "BDS" | "CHD" | "CLP"
        | "LYD" | "CRB" => (1, "Keychains"),
        _ => (0, "Stickers"),
    }
}
pub(super) fn normalize_method(code: &str, country: &str) -> String {
    let code = match country {
        "US" => "USPS(tracking&fast)",
        "TH" => "Best Express",
        "AT" | "IE" | "SE" | "LU" | "GB" | "DE" | "FR" | "PT"
            if !matches!(code, "No tracking" | "Standard Shipping") =>
        {
            "Fastship(tracking&fast)"
        }
        _ if code == "Standard Shipping" => "No tracking",
        _ => code,
    };
    match code {
        "USPS" => "USPS(tracking&fast)",
        "[NEW] TAX CLEARED SHIPPING" | "[NEW] Tax cleared shipping" => "Fastship(tracking&fast)",
        "Best express" => "Best Express",
        "41/229 Nuanchan 23 Klongkhum Beungkhum" | "Local Delivery" | "ไปรษณีย์ไทย EMS" => {
            "Headquarter"
        }
        _ => code,
    }
    .into()
}
pub(super) fn select_courier(method: &str) -> Option<Courier> {
    match method {
        "Fastship(tracking&fast)" | "USPS(tracking&fast)" => Some(Courier::Fastship),
        "Best Express" => Some(Courier::Best),
        "TELEPORT AIRASIA : TRACKING & FAST" => Some(Courier::Teleport),
        "Tracking number" | "custom" | "No tracking" | "Headquarter" | "Standard Shipping"
        | "Reship" => Some(Courier::ThaiPost),
        _ => None,
    }
}
pub(super) fn romanize(value: &str, country: &str) -> String {
    let value = if country == "JP"
        && value
            .chars()
            .any(|c| matches!(c,'\u{3400}'..='\u{9fff}'|'\u{f900}'..='\u{faff}'))
    {
        kakasi::convert(value).romaji
    } else {
        value.into()
    };
    deunicode::deunicode(&value).trim().replace('\'', "")
}
pub(super) fn joined_address(a: &ShippingAddress) -> String {
    format!(
        "{} {}",
        a.address1.as_deref().unwrap_or_default(),
        a.address2.as_deref().unwrap_or_default()
    )
}
fn required<'a>(value: Option<&'a str>, message: &'static str) -> Result<&'a str, AppError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid(message))
}
pub(super) fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, message).with_source(ClientError::new(
        ClientErrorKind::Protocol("invalid shipping data"),
    ))
}

fn known_method(code: &str) -> bool {
    matches!(
        code,
        "USPS"
            | "USPS(tracking&fast)"
            | "Fastship(tracking&fast)"
            | "[NEW] TAX CLEARED SHIPPING"
            | "[NEW] Tax cleared shipping"
            | "Best Express"
            | "Best express"
            | "Tracking number"
            | "custom"
            | "No tracking"
            | "Local Delivery"
            | "41/229 Nuanchan 23 Klongkhum Beungkhum"
            | "Headquarter"
            | "Standard Shipping"
            | "Reship"
            | "TELEPORT AIRASIA : TRACKING & FAST"
            | "ไปรษณีย์ไทย EMS"
    )
}
