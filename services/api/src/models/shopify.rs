pub use super::client::{Connection, PageInfo};
use crate::client::ClientError;
pub use crate::client::MutationUserError;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ShopifyId(String);
impl ShopifyId {
    pub fn new(value: impl Into<String>) -> Result<Self, ClientError> {
        let value = value.into();
        let id = Self(value);
        if [
            "Location",
            "Product",
            "ProductVariant",
            "InventoryItem",
            "Order",
        ]
        .iter()
        .any(|kind| id.is_kind(kind))
        {
            Ok(id)
        } else {
            Err(ClientError::InvalidInput("invalid Shopify resource ID"))
        }
    }
    pub(crate) fn require_kind(&self, kind: &str) -> Result<(), ClientError> {
        if self.is_kind(kind) {
            Ok(())
        } else {
            Err(ClientError::InvalidInput("wrong Shopify resource ID kind"))
        }
    }
    fn is_kind(&self, kind: &str) -> bool {
        self.0
            .strip_prefix(&format!("gid://shopify/{kind}/"))
            .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductStatus {
    Active,
    Draft,
    Archived,
    Unlisted,
}
impl ProductStatus {
    pub(crate) fn search_value(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Draft => "draft",
            Self::Archived => "archived",
            Self::Unlisted => "unlisted",
        }
    }
}
pub struct ListProductsRequest {
    pub location_id: ShopifyId,
    pub statuses: Vec<ProductStatus>,
}
pub struct OrdersByPaymentRequest {
    pub payment_id: String,
}
pub struct OrdersByTagRequest {
    pub tag: String,
}
pub struct SalesRangeRequest {
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
}
pub struct ReplaceOrderTagsRequest {
    pub order_id: ShopifyId,
    pub tags: Vec<String>,
}
pub struct InventoryDiscoveryRequest {
    pub location_id: ShopifyId,
}
pub struct ActivateInventoryRequest {
    pub inventory_item_id: ShopifyId,
    pub location_id: ShopifyId,
    pub idempotency_key: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Money {
    pub amount: String,
    pub currency_code: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyBag {
    pub shop_money: Money,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryQuantity {
    pub name: String,
    pub quantity: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryLevel {
    pub id: String,
    pub is_active: bool,
    pub quantities: Vec<InventoryQuantity>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantInventory {
    pub id: ShopifyId,
    pub tracked: bool,
    pub inventory_level: Option<InventoryLevel>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pricing {
    pub price: Money,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub id: ShopifyId,
    pub title: String,
    pub sku: Option<String>,
    pub contextual_pricing: Pricing,
    pub image: Option<Image>,
    pub inventory_item: VariantInventory,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: ShopifyId,
    pub title: String,
    pub description: String,
    pub status: ProductStatus,
    pub tags: Vec<String>,
    pub featured_image: Option<Image>,
    pub variants: Connection<Variant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub name: Option<String>,
    pub address1: Option<String>,
    pub address2: Option<String>,
    pub city: Option<String>,
    pub zip: Option<String>,
    pub country_code_v2: Option<String>,
    pub phone: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderTransaction {
    pub id: String,
    pub payment_id: Option<String>,
    pub gateway: Option<String>,
    pub kind: String,
    pub status: String,
    pub processed_at: Option<DateTime<FixedOffset>>,
    pub amount_set: MoneyBag,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: ShopifyId,
    pub name: String,
    pub number: i64,
    pub created_at: DateTime<FixedOffset>,
    pub display_financial_status: Option<String>,
    pub display_fulfillment_status: String,
    pub tags: Vec<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub shipping_address: Option<Address>,
    pub current_total_price_set: MoneyBag,
    pub transactions: Vec<OrderTransaction>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggedOrderLineItem {
    pub title: String,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub quantity: i64,
    pub original_unit_price_set: MoneyBag,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaggedOrderLineItems {
    pub nodes: Vec<TaggedOrderLineItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggedOrder {
    pub tags: Vec<String>,
    pub id: ShopifyId,
    pub number: i64,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub current_total_price_set: MoneyBag,
    pub line_items: TaggedOrderLineItems,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceReference {
    pub id: ShopifyId,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesLine {
    pub id: String,
    pub title: String,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub quantity: i64,
    pub current_quantity: i64,
    pub product: Option<ResourceReference>,
    pub variant: Option<ResourceReference>,
    pub original_unit_price_set: MoneyBag,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesOrder {
    pub id: ShopifyId,
    pub name: String,
    pub created_at: DateTime<FixedOffset>,
    pub cancelled_at: Option<DateTime<FixedOffset>>,
    pub test: bool,
    pub display_financial_status: Option<String>,
    pub display_fulfillment_status: String,
    pub line_items: Connection<SalesLine>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderTags {
    pub id: ShopifyId,
    pub name: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductSummary {
    pub id: ShopifyId,
    pub title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantSummary {
    pub id: ShopifyId,
    pub title: String,
    pub product: ProductSummary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryItemSummary {
    pub id: ShopifyId,
    pub sku: Option<String>,
    pub tracked: bool,
    pub variant: VariantSummary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryActivationCandidate {
    pub id: String,
    pub is_active: bool,
    pub quantities: Vec<InventoryQuantity>,
    pub item: InventoryItemSummary,
}
#[derive(Deserialize)]
pub(crate) struct ProductsData {
    pub(crate) products: Connection<Product>,
}
#[derive(Deserialize)]
pub(crate) struct OrdersData {
    pub(crate) orders: Connection<Order>,
}
#[derive(Deserialize)]
pub(crate) struct OrdersByTagData {
    pub(crate) orders: Connection<TaggedOrder>,
}
#[derive(Deserialize)]
pub(crate) struct SalesData {
    pub(crate) orders: Connection<SalesOrder>,
}
#[derive(Deserialize)]
pub(crate) struct InventoryData {
    pub(crate) location: Option<LocationInventory>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocationInventory {
    pub(crate) inventory_levels: Connection<InventoryActivationCandidate>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderUpdateData {
    pub(crate) order_update: OrderUpdate,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderUpdate {
    pub(crate) order: Option<OrderTags>,
    pub(crate) user_errors: Vec<MutationUserError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivationData {
    pub(crate) inventory_activate: Activation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Activation {
    pub(crate) inventory_level: Option<InventoryLevel>,
    pub(crate) user_errors: Vec<MutationUserError>,
}
#[cfg(test)]
#[path = "../tests/models/shopify.rs"]
mod tests;
