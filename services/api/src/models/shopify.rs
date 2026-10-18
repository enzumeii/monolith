use crate::client::ClientError;
pub use crate::client::MutationUserError;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection<T> {
    pub nodes: Vec<T>,
    pub page_info: PageInfo,
}

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
    pub tags: Vec<String>,
}
pub struct OrderLinesRequest {
    pub order_id: ShopifyId,
}
impl OrderLinesRequest {
    pub(crate) fn variables(&self) -> Result<Value, ClientError> {
        self.order_id.require_kind("Order")?;
        Ok(json!({"id":self.order_id}))
    }
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

impl ListProductsRequest {
    pub(crate) fn variables(&self, page: &PageRequest) -> Result<Value, ClientError> {
        page.validate()?;
        self.location_id.require_kind("Location")?;
        let search = if self.statuses.is_empty() {
            None
        } else {
            Some(format!(
                "status:{}",
                self.statuses
                    .iter()
                    .map(|s| s.search_value())
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        };
        Ok(
            json!({"locationId": self.location_id, "search": search, "after": page.after, "first": page.first}),
        )
    }
}
impl OrdersByPaymentRequest {
    pub(crate) fn variables(&self, page: &PageRequest) -> Result<Value, ClientError> {
        page.validate()?;
        if self.payment_id.trim().is_empty() {
            return Err(ClientError::InvalidInput("payment ID is empty"));
        }
        Ok(
            json!({"search": format!("payment_id:\"{}\"", escape_search(&self.payment_id)), "after": page.after, "first": page.first}),
        )
    }
}
impl OrdersByTagRequest {
    pub(crate) fn variables(&self, page: &PageRequest) -> Result<Value, ClientError> {
        page.validate()?;
        if self.tags.is_empty() || self.tags.iter().any(|tag| tag.trim().is_empty()) {
            return Err(ClientError::InvalidInput("order tag is empty"));
        }
        let search = self
            .tags
            .iter()
            .map(|tag| format!("tag:\"{}\"", escape_search(tag)))
            .collect::<Vec<_>>()
            .join(" AND ");
        Ok(json!({"search": search, "after": page.after, "first": page.first}))
    }
}
impl SalesRangeRequest {
    pub(crate) fn variables(&self, page: &PageRequest) -> Result<Value, ClientError> {
        page.validate()?;
        if self.start >= self.end {
            return Err(ClientError::InvalidInput("sales start must precede end"));
        }
        Ok(
            json!({"search": format!("created_at:>='{}' created_at:<'{}'", self.start.to_rfc3339(), self.end.to_rfc3339()), "ordersAfter": page.after, "first": page.first}),
        )
    }
}
impl ReplaceOrderTagsRequest {
    pub(crate) fn variables(&self) -> Result<Value, ClientError> {
        self.order_id.require_kind("Order")?;
        Ok(json!({"input": {"id": self.order_id, "tags": self.tags}}))
    }
}
impl InventoryDiscoveryRequest {
    pub(crate) fn variables(&self, page: &PageRequest) -> Result<Value, ClientError> {
        page.validate()?;
        self.location_id.require_kind("Location")?;
        Ok(json!({"locationId": self.location_id, "after": page.after, "first": page.first}))
    }
}
impl ActivateInventoryRequest {
    pub(crate) fn variables(&self) -> Result<Value, ClientError> {
        self.inventory_item_id.require_kind("InventoryItem")?;
        self.location_id.require_kind("Location")?;
        Ok(
            json!({"inventoryItemId": self.inventory_item_id, "locationId": self.location_id, "idempotencyKey": self.idempotency_key}),
        )
    }
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
    #[serde(flatten)]
    pub metadata: InventoryMetadata,
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
    pub sku: Option<Sku>,
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
    pub product_type: String,
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
    pub sku: Option<Sku>,
    pub quantity: i64,
    pub original_unit_price_set: MoneyBag,
    pub product: Option<LineProduct>,
    pub variant: Option<LineVariant>,
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
    pub line_items: Connection<TaggedOrderLineItem>,
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
    pub sku: Option<Sku>,
    pub quantity: i64,
    pub current_quantity: i64,
    pub product: Option<ResourceReference>,
    pub variant: Option<ResourceReference>,
    pub original_unit_price_set: MoneyBag,
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
    pub sku: Option<Sku>,
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
#[serde(rename_all = "camelCase")]
pub struct SalesOrderHeader {
    pub id: ShopifyId,
    pub name: String,
    pub created_at: DateTime<FixedOffset>,
    pub cancelled_at: Option<DateTime<FixedOffset>>,
    pub test: bool,
    pub display_financial_status: Option<String>,
    pub display_fulfillment_status: String,
}

#[derive(Deserialize)]
pub struct ProductsData {
    pub products: Connection<Product>,
}
#[derive(Deserialize)]
pub struct OrdersData {
    pub orders: Connection<Order>,
}
#[derive(Deserialize)]
pub struct OrdersByTagData {
    pub orders: Connection<TaggedOrder>,
}
#[derive(Deserialize)]
pub struct SalesData {
    pub orders: Connection<SalesOrderHeader>,
}
#[derive(Deserialize)]
pub struct InventoryData {
    pub location: Option<LocationInventory>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInventory {
    pub inventory_levels: Connection<InventoryActivationCandidate>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdateData {
    pub order_update: OrderUpdate,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdate {
    pub order: Option<OrderTags>,
    pub user_errors: Vec<MutationUserError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationData {
    pub inventory_activate: Activation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    pub inventory_level: Option<InventoryLevel>,
    pub user_errors: Vec<MutationUserError>,
}
#[cfg(test)]
#[path = "../tests/models/shopify.rs"]
mod tests;

#[derive(Deserialize)]
pub struct OrderLinesData<T> {
    pub order: Option<OrderLines<T>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLines<T> {
    pub id: ShopifyId,
    pub line_items: Connection<T>,
}
fn escape_search(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        if matches!(character, '\\' | '"' | ':' | '(' | ')') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub weight: Option<Weight>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weight {
    pub value: serde_json::Number,
    pub unit: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineVariant {
    pub inventory_item: InventoryMetadata,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMetadata {
    pub unit_cost: Option<Money>,
    pub measurement: Measurement,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineProduct {
    pub product_type: String,
}
#[derive(Deserialize)]
pub struct LocationData {
    pub location: Option<ResourceReference>,
}

pub struct PageRequest {
    pub first: u16,
    pub after: Option<String>,
}
impl PageRequest {
    pub fn validate(&self) -> Result<(), ClientError> {
        validate_size(self.first)?;
        if self
            .after
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ClientError::InvalidInput("cursor is empty"));
        }
        Ok(())
    }
}
impl PageInfo {
    pub fn validate_after(&self, after: Option<&str>) -> Result<(), ClientError> {
        if self.has_next_page
            && self
                .end_cursor
                .as_deref()
                .is_none_or(|next| next.trim().is_empty() || Some(next) == after)
        {
            return Err(
                ClientError::Protocol("next cursor is missing or did not progress")
                    .at("api::models::shopify"),
            );
        }
        Ok(())
    }
}
fn validate_size(first: u16) -> Result<(), ClientError> {
    if !(1..=250).contains(&first) {
        return Err(ClientError::InvalidInput(
            "page size must be between 1 and 250",
        ));
    }
    Ok(())
}
mod products;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sku(String);
impl Sku {
    #[must_use]
    pub fn collection(&self) -> Option<String> {
        let value = self.0.chars().skip(3).take(6).collect::<String>();
        (value.chars().count() == 6).then_some(value)
    }
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl<T> Connection<T> {
    pub(crate) fn into_complete_nodes(self) -> Result<Vec<T>, ClientError> {
        if self.page_info.has_next_page {
            return Err(
                ClientError::Protocol("nested connection exceeds report limit")
                    .at("api::models::shopify"),
            );
        }
        Ok(self.nodes)
    }
}
