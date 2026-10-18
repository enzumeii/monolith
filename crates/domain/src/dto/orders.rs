use super::ProductReportRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderReportQuery {
    pub tags: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrderReportResponse {
    pub products: Vec<ProductReportRow>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListOrdersQuery {
    pub tag: String,
    pub fulfillment: Option<FulfillmentStatus>,
    pub page_size: Option<u16>,
    pub cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentStatus {
    Unfulfilled,
    Fulfilled,
    Partial,
    Unshipped,
    Shipped,
    Any,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrderSummary {
    pub id: u64,
    pub number: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrderPageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListOrdersResponse {
    pub orders: Vec<OrderSummary>,
    pub page_info: OrderPageInfo,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Courier {
    Fastship,
    ThaiPost,
    Best,
    Teleport,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FulfillOrderRequest {
    pub courier: Courier,
    pub tracking_number: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FulfillOrderResponse {
    pub success: bool,
    pub tracking_url: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ShippingLabels {
    pub fastship: Vec<FastshipLabel>,
    pub thaipost: Vec<ThaiPostLabel>,
    pub best: Vec<BestLabel>,
    pub teleport: Vec<TeleportLabel>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FastshipLabel {
    pub firstname: String,
    pub lastname: String,
    pub email: String,
    pub phone: String,
    pub address_1: String,
    pub address_2: String,
    pub city: String,
    pub state: String,
    pub postcode: String,
    pub country: String,
    pub width: u32,
    pub height: u32,
    pub length: u32,
    pub weight: u64,
    pub category: String,
    pub quantity: u64,
    pub declared_value: serde_json::Number,
    pub ioss: String,
    pub note: String,
    pub shipping_option: String,
    pub reference_id: String,
    pub incoterm: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ThaiPostLabel {
    pub country_code: String,
    pub weight: u64,
    pub service_id: u32,
    pub category: String,
    pub sender_name: String,
    pub sender_phone: String,
    pub sender_email: String,
    pub sender_address: String,
    pub sender_sub_district: String,
    pub sender_district: String,
    pub sender_province: String,
    pub sender_postcode: String,
    pub sender_country_code: String,
    pub receiver_name: String,
    pub receiver_phone: String,
    pub receiver_email: String,
    pub receiver_address: String,
    pub receiver_state: String,
    pub receiver_city: String,
    pub receiver_postcode: String,
    pub currency: String,
    pub item_description: String,
    pub quantity: u64,
    pub price: serde_json::Number,
    pub origin: String,
    pub ioss: String,
    pub order_id: String,
    pub insurance: String,
    pub hs_code: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BestLabel {
    pub name: String,
    pub phone: String,
    pub zip_code: String,
    pub address: String,
    pub category: String,
    pub cod_amount: String,
    pub insured_amount: u64,
    pub waybill_number: String,
    pub note: String,
    pub parcel_id: String,
    pub weight: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TeleportLabel {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub address: String,
    pub state: String,
    pub city: String,
    pub postal_code: String,
    pub weight: serde_json::Number,
    pub is_multi_piece: String,
    pub quantity: u64,
    pub commodity_value: serde_json::Number,
    pub package: String,
    pub length: u32,
    pub width: u32,
    pub height: u32,
    pub commodity: String,
    pub commodity_description: String,
    pub commodity_currency: String,
    pub delivery_instructions: String,
    pub tracking_number: String,
    pub reference: String,
    pub destination_geo: String,
}
