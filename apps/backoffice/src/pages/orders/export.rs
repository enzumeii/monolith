use super::data::{FulfillmentOrder, FulfillmentResult};
use domain::dto::orders::{FastshipLabel, ThaiPostLabel};

#[derive(Clone, PartialEq)]
pub struct TableData {
    pub headers: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}

const FULFILLMENT_HEADERS: &[&str] = &[
    "order_number",
    "customer_name",
    "email",
    "country",
    "courier",
    "tracking_number",
    "result",
    "tracking_url",
];

const FASTSHIP_HEADERS: &[&str] = &[
    "firstname",
    "lastname",
    "email",
    "phone",
    "address_1",
    "address_2",
    "city",
    "state",
    "postcode",
    "country",
    "width",
    "height",
    "length",
    "weight",
    "category",
    "quantity",
    "declared_value",
    "ioss",
    "note",
    "shipping_option",
    "reference_id",
    "incoterm",
];

const THAI_POST_HEADERS: &[&str] = &[
    "country_code",
    "weight",
    "service_id",
    "category",
    "sender_name",
    "sender_phone",
    "sender_email",
    "sender_address",
    "sender_sub_district",
    "sender_district",
    "sender_province",
    "sender_postcode",
    "sender_country_code",
    "receiver_name",
    "receiver_phone",
    "receiver_email",
    "receiver_address",
    "receiver_state",
    "receiver_city",
    "receiver_postcode",
    "currency",
    "item_description",
    "quantity",
    "price",
    "origin",
    "ioss",
    "order_id",
    "insurance",
    "hs_code",
];

pub fn fulfillment_table(
    orders: &[FulfillmentOrder],
    tracking_numbers: &std::collections::HashMap<u64, String>,
    results: &[FulfillmentResult],
) -> TableData {
    TableData {
        headers: FULFILLMENT_HEADERS,
        rows: orders
            .iter()
            .map(|row| {
                let result = results
                    .iter()
                    .find(|result| result.order_number == row.order.number);
                let tracking_number = result
                    .map(|result| result.tracking_number.as_str())
                    .or_else(|| tracking_numbers.get(&row.order.id).map(String::as_str))
                    .unwrap_or_default();
                let (status, tracking_url) =
                    result.map_or(("", ""), |result| match &result.result {
                        Ok(response) if response.success => {
                            ("Fulfilled", response.tracking_url.as_str())
                        }
                        Ok(response) => ("Failed", response.tracking_url.as_str()),
                        Err(_) => ("Failed", ""),
                    });
                vec![
                    row.order.number.to_string(),
                    row.order.full_name.clone().unwrap_or_default(),
                    row.order.email.clone().unwrap_or_default(),
                    row.order.shipping_country_code.clone().unwrap_or_default(),
                    courier_name(row.courier).to_owned(),
                    tracking_number.to_owned(),
                    status.to_owned(),
                    tracking_url.to_owned(),
                ]
            })
            .collect(),
    }
}

pub fn fastship_table(labels: &[FastshipLabel]) -> TableData {
    TableData {
        headers: FASTSHIP_HEADERS,
        rows: labels
            .iter()
            .map(|label| {
                vec![
                    label.firstname.clone(),
                    label.lastname.clone(),
                    label.email.clone(),
                    label.phone.clone(),
                    label.address_1.clone(),
                    label.address_2.clone(),
                    label.city.clone(),
                    label.state.clone(),
                    label.postcode.clone(),
                    label.country.clone(),
                    label.width.to_string(),
                    label.height.to_string(),
                    label.length.to_string(),
                    label.weight.to_string(),
                    label.category.clone(),
                    label.quantity.to_string(),
                    label.declared_value.to_string(),
                    label.ioss.clone(),
                    label.note.clone(),
                    label.shipping_option.clone(),
                    label.reference_id.clone(),
                    label.incoterm.clone(),
                ]
            })
            .collect(),
    }
}

pub fn thai_post_table(labels: &[ThaiPostLabel]) -> TableData {
    TableData {
        headers: THAI_POST_HEADERS,
        rows: labels
            .iter()
            .map(|label| {
                vec![
                    label.country_code.clone(),
                    label.weight.to_string(),
                    label.service_id.to_string(),
                    label.category.clone(),
                    label.sender_name.clone(),
                    label.sender_phone.clone(),
                    label.sender_email.clone(),
                    label.sender_address.clone(),
                    label.sender_sub_district.clone(),
                    label.sender_district.clone(),
                    label.sender_province.clone(),
                    label.sender_postcode.clone(),
                    label.sender_country_code.clone(),
                    label.receiver_name.clone(),
                    label.receiver_phone.clone(),
                    label.receiver_email.clone(),
                    label.receiver_address.clone(),
                    label.receiver_state.clone(),
                    label.receiver_city.clone(),
                    label.receiver_postcode.clone(),
                    label.currency.clone(),
                    label.item_description.clone(),
                    label.quantity.to_string(),
                    label.price.to_string(),
                    label.origin.clone(),
                    label.ioss.clone(),
                    label.order_id.clone(),
                    label.insurance.clone(),
                    label.hs_code.clone(),
                ]
            })
            .collect(),
    }
}

fn courier_name(courier: domain::dto::orders::Courier) -> &'static str {
    match courier {
        domain::dto::orders::Courier::Fastship => "FASTSHIP",
        domain::dto::orders::Courier::ThaiPost => "THAI_POST",
        _ => "",
    }
}

#[cfg(test)]
#[path = "../../tests/pages/orders/export.rs"]
mod tests;
