use super::data::{FulfillmentOrder, FulfillmentResult};
use domain::dto::orders::{BestLabel, FastshipLabel, ShippingLabels, TeleportLabel, ThaiPostLabel};

use crate::components::TableData;

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
    "Firstname",
    "Lastname",
    "Email",
    "Phone",
    "Address Line 1",
    "Address Line 2",
    "City",
    "State",
    "Postcode",
    "Country",
    "Width",
    "Height",
    "Length",
    "Weight",
    "Category",
    "Qty",
    "Declare Value",
    "IOSS",
    "Note",
    "Shipping Option",
    "ReferenceId",
    "Incoterm",
];

const THAI_POST_HEADERS: &[&str] = &[
    "รหัสประเทศ (Country Code)",
    "น้ำหนักรวม (Total Weight (g.))",
    "รหัสบริการ (Service ID)",
    "ประเภทสิ่งของ (Category)",
    "ชื่อ-นามสกุลผู้ฝากส่ง (Sender Name)",
    "เบอร์โทรศัพท์ผู้ฝากส่ง (Tel.)",
    "อีเมล (E-mail) ผู้ฝากส่ง",
    "ที่อยู่ผู้ฝากส่ง (Sender Address)",
    "แขวง (Sub-District)",
    "เขต (District)",
    "จังหวัด (Province)",
    "รหัสประเทศ (Postcode) ",
    "รหัสประเทศ (Country Code) ",
    "ชื่อ-นามสกุลผู้รับ (Receiver Name)",
    "เบอร์โทรศัพท์ผู้รับ (Tel.)",
    "อีเมล (E-mail) ผู้รับ",
    "ที่อยู่ผู้รับ (Receiver Address)",
    "เขต (State/Province)",
    "เมือง (City/District)",
    "รหัสประเทศ (Postcode)",
    "ค่าเงิน (Currency)",
    "สิ่งของภายใน (Item Description)",
    "จำนวน (Quantity)",
    "น้ำหนัก (Net Weight (g.))",
    "มูลค่า (Price)",
    "ประเทศกำเนิดสิ่งของ (Origin)",
    "IOSS Number",
    "ORDER_ID",
    "INSURANCE",
    "พิกัดศุลกากร (HS Code)",
];

const BEST_HEADERS: &[&str] = &[
    "*Recipient's name",
    "*Recipient's telephone#",
    "*Recipient's post code",
    "*Recipient's address detail",
    "*Commodity",
    "COD amount",
    "Insured amount",
    "Waybill#",
    "Note",
    "parcel ID",
    "Weight",
];

const TELEPORT_HEADERS: &[&str] = &[
    "Recipient Name*",
    "Recipient Email*",
    "Recipient Phone No*",
    "Recipient Address*",
    "Recipient State*",
    "Recipient City*",
    "Recipient Postal Code*",
    "Package*",
    "Length (cm)*",
    "Width (cm)*",
    "Height (cm)*",
    "Weight (kg)*",
    "Multi Piece/ Single Piece*",
    "Quantity ( pieces)*",
    "Commodity*",
    "Commodity Description*",
    "Commodity Currency*",
    "Commodity Value*",
    "Delivery Instruction (optional)",
    "Tracking Number (optional)",
    "Reference  (optional)",
    "Destination geo  (optional)",
];

pub fn shipping_label_archive(labels: &ShippingLabels) -> Result<Vec<u8>, String> {
    let mut files = Vec::new();
    for (name, sheet, table, format) in [
        (
            "fastship-labels.csv",
            "Fastship",
            fastship_table(&labels.fastship),
            crate::export::ExportFormat::Csv,
        ),
        (
            "thai-post-labels.xlsx",
            "ThaiPost",
            thai_post_table(&labels.thaipost),
            crate::export::ExportFormat::Xlsx,
        ),
        (
            "best-labels.xlsx",
            "Best",
            best_table(&labels.best),
            crate::export::ExportFormat::Xlsx,
        ),
        (
            "teleport-labels.xlsx",
            "Teleport",
            teleport_table(&labels.teleport),
            crate::export::ExportFormat::Xlsx,
        ),
    ] {
        if table.rows.is_empty() {
            continue;
        }
        let bytes = match format {
            crate::export::ExportFormat::Csv => {
                crate::export::encode_table(table.headers, &table.rows).into_bytes()
            }
            crate::export::ExportFormat::Xlsx => {
                crate::export::encode_table_xlsx(table.headers, &table.rows, sheet)?
            }
        };
        files.push((name, bytes));
    }
    crate::export::encode_archive(&files)
}

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
                    row.courier.tag().to_owned(),
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
                    label.weight.to_string(),
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

pub fn best_table(labels: &[BestLabel]) -> TableData {
    TableData {
        headers: BEST_HEADERS,
        rows: labels
            .iter()
            .map(|label| {
                vec![
                    label.name.clone(),
                    label.phone.clone(),
                    label.zip_code.clone(),
                    label.address.clone(),
                    label.category.clone(),
                    label.cod_amount.clone(),
                    label.insured_amount.to_string(),
                    label.waybill_number.clone(),
                    label.note.clone(),
                    label.parcel_id.clone(),
                    label.weight.to_string(),
                ]
            })
            .collect(),
    }
}

pub fn teleport_table(labels: &[TeleportLabel]) -> TableData {
    TableData {
        headers: TELEPORT_HEADERS,
        rows: labels
            .iter()
            .map(|label| {
                vec![
                    label.name.clone(),
                    label.email.clone(),
                    label.phone.clone(),
                    label.address.clone(),
                    label.state.clone(),
                    label.city.clone(),
                    label.postal_code.clone(),
                    label.package.clone(),
                    label.length.to_string(),
                    label.width.to_string(),
                    label.height.to_string(),
                    label.weight.to_string(),
                    label.is_multi_piece.clone(),
                    label.quantity.to_string(),
                    label.commodity.clone(),
                    label.commodity_description.clone(),
                    label.commodity_currency.clone(),
                    label.commodity_value.to_string(),
                    label.delivery_instructions.clone(),
                    label.tracking_number.clone(),
                    label.reference.clone(),
                    label.destination_geo.clone(),
                ]
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "../../tests/pages/orders/export.rs"]
mod tests;
