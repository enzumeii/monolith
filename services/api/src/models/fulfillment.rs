use super::shopify::{Connection, MoneyBag};
use crate::client::{ClientError, ClientErrorKind, MutationUserError};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct OrdersPage<T> {
    pub orders: Connection<T>,
}
#[derive(Deserialize)]
pub struct OrderSummary {
    pub id: String,
    pub number: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingOrder {
    pub id: String,
    pub email: Option<String>,
    pub currency_code: String,
    #[serde(default, deserialize_with = "deserialize_weight")]
    pub total_weight: Option<u64>,
    pub shipping_address: Option<ShippingAddress>,
    pub shipping_lines: Connection<ShippingLine>,
    pub line_items: Connection<ShippingItem>,
}
#[derive(Deserialize)]
pub struct ShippingLine {
    pub code: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingItem {
    pub title: String,
    pub sku: Option<String>,
    pub quantity: u64,
    pub original_unit_price_set: MoneyBag,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingAddress {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub name: Option<String>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub city: Option<String>,
    pub province_code: Option<String>,
    pub zip: Option<String>,
    pub country: Option<String>,
    pub country_code_v2: Option<String>,
    pub phone: Option<String>,
}
#[derive(Deserialize)]
pub struct OrderData {
    #[serde(deserialize_with = "Option::deserialize")]
    pub order: Option<FulfillmentOrderHeader>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentOrderHeader {
    pub id: String,
    pub fulfillment_orders: Connection<FulfillmentOrder>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentOrder {
    pub id: String,
    pub status: String,
    pub assigned_location: AssignedLocation,
    pub supported_actions: Vec<SupportedAction>,
    pub line_items: Connection<FulfillmentLine>,
}
#[derive(Deserialize)]
pub struct SupportedAction {
    pub action: String,
}
#[derive(Deserialize)]
pub struct AssignedLocation {
    pub location: Option<Location>,
}
#[derive(Deserialize)]
pub struct Location {
    pub id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentLine {
    pub id: String,
    pub remaining_quantity: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagData {
    #[serde(alias = "tagsRemove")]
    pub tags_add: TagResult,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagResult {
    pub node: Option<Location>,
    pub user_errors: Vec<MutationUserError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateData {
    pub fulfillment_create: CreateResult,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    pub fulfillment: Option<Location>,
    pub user_errors: Vec<MutationUserError>,
}

pub fn resource_number(id: &str, kind: &str) -> Result<u64, ClientError> {
    id.strip_prefix(&format!("gid://shopify/{kind}/"))
        .filter(|value| !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()))
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| ClientError::new(ClientErrorKind::Protocol("invalid Shopify resource ID")))
}

pub fn mutation_result(
    errors: Vec<MutationUserError>,
    node: Option<Location>,
) -> Result<String, ClientError> {
    if !errors.is_empty() {
        return Err(ClientError::new(ClientErrorKind::Mutation { errors }));
    }
    node.map(|node| node.id)
        .ok_or_else(|| ClientError::new(ClientErrorKind::Protocol("mutation result missing")))
}

#[cfg(test)]
#[path = "../tests/models/fulfillment.rs"]
mod tests;

fn deserialize_weight<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Weight {
        Integer(u64),
        Text(String),
    }
    Option::<Weight>::deserialize(deserializer)?
        .map(|weight| match weight {
            Weight::Integer(value) => Ok(value),
            Weight::Text(value) => value.parse().map_err(serde::de::Error::custom),
        })
        .transpose()
}

#[derive(Deserialize)]
pub struct ShippingItemsData {
    #[serde(deserialize_with = "Option::deserialize")]
    pub order: Option<OrderItems>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderItems {
    pub id: String,
    pub line_items: Connection<ShippingItem>,
}
#[derive(Deserialize)]
pub struct ShippingMethodsData {
    #[serde(deserialize_with = "Option::deserialize")]
    pub order: Option<OrderMethods>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderMethods {
    pub id: String,
    pub shipping_lines: Connection<ShippingLine>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentItemsData {
    #[serde(deserialize_with = "Option::deserialize")]
    pub fulfillment_order: Option<FulfillmentItems>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentItems {
    pub id: String,
    pub line_items: Connection<FulfillmentLine>,
}
