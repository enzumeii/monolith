use super::common::{Parcel, joined_address};
use domain::dto::orders::BestLabel;

impl Parcel {
    pub(super) fn best(self) -> BestLabel {
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
}
