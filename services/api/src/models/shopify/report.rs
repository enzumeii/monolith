use super::{Connection, InventoryLevel, Money, ShopifyId};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInventoryItem {
    pub unit_cost: Option<Money>,
    pub measurement: Measurement,
    pub inventory_level: Option<InventoryLevel>,
}
#[derive(Deserialize)]
pub struct Measurement {
    pub weight: Option<Weight>,
}
#[derive(Deserialize)]
pub struct Weight {
    pub value: serde_json::Number,
    pub unit: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportVariant {
    pub title: String,
    pub sku: Option<String>,
    pub contextual_pricing: super::Pricing,
    pub inventory_item: ReportInventoryItem,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProduct {
    pub id: ShopifyId,
    pub title: String,
    pub product_type: String,
    pub variants: Connection<ReportVariant>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineVariant {
    pub inventory_item: LineInventoryItem,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineInventoryItem {
    pub unit_cost: Option<Money>,
    pub measurement: Measurement,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineProduct {
    pub product_type: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportLine {
    pub title: String,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub quantity: i64,
    pub original_unit_price_set: super::MoneyBag,
    pub product: Option<LineProduct>,
    pub variant: Option<LineVariant>,
}
#[derive(Deserialize)]
pub struct InventoryReportData {
    pub location: Option<super::ResourceReference>,
    pub products: Connection<ReportProduct>,
}
#[derive(Deserialize)]
pub struct VariantPageData {
    pub product: Option<VariantPage>,
}
#[derive(Deserialize)]
pub struct VariantPage {
    pub id: ShopifyId,
    pub variants: Connection<ReportVariant>,
}
