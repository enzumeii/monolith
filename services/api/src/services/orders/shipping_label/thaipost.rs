use super::common::{Parcel, joined_address};
use crate::config::ShippingSender;
use domain::dto::orders::ThaiPostLabel;

impl Parcel {
    pub(super) fn thaipost(self, s: &ShippingSender) -> ThaiPostLabel {
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
}
