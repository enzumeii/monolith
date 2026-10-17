use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductReportType {
    Inventory,
    OrderTag,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductReportQuery {
    #[serde(rename = "type")]
    pub report_type: ProductReportType,
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProductsResponse {
    pub products: Vec<ProductReportRow>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ProductReportRow {
    pub category: Option<String>,
    pub collection: Option<String>,
    pub sku: Option<String>,
    pub title: String,
    pub variant: Option<String>,
    pub cost: Option<Number>,
    pub cost_currency: Option<String>,
    pub weight: Option<Number>,
    pub unit: Option<String>,
    pub selling_price: Number,
    pub selling_price_currency: String,
    pub quantity: i64,
    pub total_selling_price: Number,
    pub total_weight: Option<Number>,
}
