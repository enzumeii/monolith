use super::common::Parcel;
use domain::dto::orders::FastshipLabel;

impl Parcel {
    pub(super) fn fastship(self) -> FastshipLabel {
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
}
