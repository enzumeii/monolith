use super::common::{Parcel, invalid, joined_address};
use crate::error::AppError;
use domain::dto::orders::TeleportLabel;
use serde_json::Number;

impl Parcel {
    pub(super) fn teleport(self) -> Result<TeleportLabel, AppError> {
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
