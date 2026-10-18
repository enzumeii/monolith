use crate::{
    client::{ClientError, ClientErrorKind},
    config::ShippingSender,
    error::AppError,
    models::fulfillment::{ShippingAddress, ShippingItem, ShippingOrder},
};
use axum::http::StatusCode;
use domain::dto::orders::{BestLabel, FastshipLabel, ShippingLabels, TeleportLabel, ThaiPostLabel};
use serde_json::Number;

struct Parcel {
    address: ShippingAddress,
    country_code: String,
    email: String,
    weight: u64,
    quantity: u64,
    price: Number,
    category: String,
    method: String,
}

pub(super) fn append_label(
    order: ShippingOrder,
    sender: Option<&ShippingSender>,
    labels: &mut ShippingLabels,
) -> Result<&'static str, AppError> {
    let parcel = Parcel::from_order(order)?;
    let courier = select_courier(&parcel.method);
    match courier {
        "FASTSHIP" => labels.fastship.push(parcel.fastship()),
        "THAI_POST" => labels.thaipost.push(parcel.thaipost(sender.ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shipping sender unavailable",
            )
        })?)),
        "BEST" => labels.best.push(parcel.best()),
        "TELEPORT" => labels.teleport.push(parcel.teleport()?),
        _ => {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "Order has no supported shipping method",
            ));
        }
    }
    Ok(courier)
}

impl Parcel {
    fn from_order(order: ShippingOrder) -> Result<Self, AppError> {
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
            .map_err(|error| super::super::common::upstream(error))?;
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
        let method = normalize_method(code, &country_code);
        let (quantity, price, category) = declaration(
            order
                .line_items
                .into_complete_nodes()
                .map_err(|error| super::super::common::upstream(error))?,
            &order.currency_code,
        )?;
        let weight = if select_courier(&method) == "BEST" {
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
                .unwrap_or_else(|| "automated.enzumeii@gmail.com".into()),
            weight,
            quantity,
            price,
            category,
            method,
        })
    }
    fn fastship(self) -> FastshipLabel {
        let a = self.address;
        FastshipLabel {
            firstname: a.first_name.unwrap_or_default(),
            lastname: a.last_name.unwrap_or_default(),
            email: self.email,
            phone: a.phone.unwrap_or_default(),
            address_1: a.address1.unwrap_or_default(),
            address_2: a.address2.unwrap_or_default(),
            city: a.city.unwrap_or_default(),
            state: a
                .province_code
                .unwrap_or_else(|| a.country.clone().unwrap_or_default()),
            postcode: a.zip.unwrap_or_default(),
            country: a.country.unwrap_or_default(),
            width: 0,
            height: 0,
            length: 0,
            weight: self.weight,
            category: self.category,
            quantity: self.quantity,
            declared_value: self.price,
            ioss: String::new(),
            note: String::new(),
            shipping_option: String::new(),
            reference_id: String::new(),
            incoterm: if self.country_code == "US" {
                "DDU".into()
            } else {
                String::new()
            },
        }
    }
    fn thaipost(self, s: &ShippingSender) -> ThaiPostLabel {
        let service_id = if self.weight >= 2000 {
            3278
        } else if matches!(
            self.method.as_str(),
            "Tracking number" | "USPS(tracking&fast)" | "Fastship(tracking&fast)"
        ) {
            358_702
        } else {
            358_701
        };
        let address = joined_address(&self.address);
        let a = self.address;
        ThaiPostLabel {
            country_code: self.country_code,
            weight: self.weight,
            service_id,
            category: "Merchandise".into(),
            sender_name: s.name.clone(),
            sender_phone: s.phone.clone(),
            sender_email: s.email.clone(),
            sender_address: s.address.clone(),
            sender_sub_district: s.sub_district.clone(),
            sender_district: s.district.clone(),
            sender_province: s.province.clone(),
            sender_postcode: s.postcode.clone(),
            sender_country_code: s.country_code.clone(),
            receiver_name: a.name.unwrap_or_default(),
            receiver_phone: a.phone.unwrap_or_default(),
            receiver_email: self.email,
            receiver_address: address,
            receiver_state: a.province_code.unwrap_or_default(),
            receiver_city: a.city.unwrap_or_default(),
            receiver_postcode: a.zip.unwrap_or_else(|| "0".into()),
            currency: "THB".into(),
            item_description: self.category,
            quantity: self.quantity,
            price: self.price,
            origin: "TH".into(),
            ioss: String::new(),
            order_id: String::new(),
            insurance: String::new(),
            hs_code: "3926400000".into(),
        }
    }
    fn best(self) -> BestLabel {
        let address = format!(
            "{}, {}, {}, {}, {}",
            joined_address(&self.address),
            self.address.city.as_deref().unwrap_or_default(),
            self.address.province_code.as_deref().unwrap_or_default(),
            self.address.country.as_deref().unwrap_or_default(),
            self.address.zip.as_deref().unwrap_or_default()
        );
        BestLabel {
            name: self.address.name.unwrap_or_default(),
            phone: self.address.phone.unwrap_or_default(),
            zip_code: self.address.zip.unwrap_or_default(),
            address,
            category: self.category,
            cod_amount: String::new(),
            insured_amount: 0,
            waybill_number: String::new(),
            note: String::new(),
            parcel_id: String::new(),
            weight: 0,
        }
    }
    fn teleport(self) -> Result<TeleportLabel, AppError> {
        let address = joined_address(&self.address);
        let weight = self
            .weight
            .to_string()
            .parse::<f64>()
            .map_err(|_| invalid("Invalid shipping weight"))?
            / 1000.0;
        let weight = Number::from_f64(weight).ok_or_else(|| invalid("Invalid shipping weight"))?;
        let a = self.address;
        Ok(TeleportLabel {
            name: a.name.unwrap_or_default(),
            email: self.email,
            phone: a.phone.unwrap_or_default(),
            address,
            state: a
                .province_code
                .unwrap_or_else(|| a.country.unwrap_or_default()),
            city: a.city.unwrap_or_default(),
            postal_code: a.zip.unwrap_or_default(),
            weight,
            is_multi_piece: if self.quantity > 1 {
                "Multi Piece Shippment".into()
            } else {
                "Single Piece Shippment".into()
            },
            quantity: self.quantity,
            commodity_value: self.price,
            package: "Polymailer".into(),
            length: 13,
            width: 15,
            height: 4,
            commodity: String::new(),
            commodity_description: String::new(),
            commodity_currency: "THB".into(),
            delivery_instructions: "Please call customer before delivery".into(),
            tracking_number: String::new(),
            reference: String::new(),
            destination_geo: String::new(),
        })
    }
}

fn declaration(
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
fn normalize_method(code: &str, country: &str) -> String {
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
fn select_courier(method: &str) -> &'static str {
    match method {
        "Fastship(tracking&fast)" | "USPS(tracking&fast)" => "FASTSHIP",
        "Best Express" => "BEST",
        "TELEPORT AIRASIA : TRACKING & FAST" => "TELEPORT",
        _ => "THAI_POST",
    }
}
fn romanize(value: &str, country: &str) -> String {
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
fn joined_address(a: &ShippingAddress) -> String {
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
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, message).with_source(ClientError::new(
        ClientErrorKind::Protocol("invalid shipping data"),
    ))
}

#[cfg(test)]
#[path = "../../tests/services/orders/shipping.rs"]
mod tests;
