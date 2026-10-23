use super::PageInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueQuery {
    pub location_id: String,
    pub page_size: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CatalogueResponse {
    pub products: Vec<CatalogueProduct>,
    pub page_info: PageInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CatalogueProduct {
    pub id: String,
    pub title: String,
    pub description: String,
    pub featured_image: Option<CatalogueImage>,
    pub variants: Vec<CatalogueVariant>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CatalogueImage {
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CatalogueOption {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CataloguePrice {
    pub amount: String,
    pub currency_code: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CatalogueInventoryPolicy {
    Deny,
    Continue,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CatalogueVariant {
    pub id: String,
    pub inventory_item_id: String,
    pub title: String,
    pub sku: Option<String>,
    pub image: Option<CatalogueImage>,
    pub selected_options: Vec<CatalogueOption>,
    pub price: CataloguePrice,
    pub tracked: bool,
    pub inventory_policy: CatalogueInventoryPolicy,
    pub available_quantity: Option<i64>,
    pub eligible: bool,
}
