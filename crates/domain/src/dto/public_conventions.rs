use super::{CatalogueImage, CatalogueInventoryPolicy, CatalogueOption};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PublicConvention {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub logo_preview_url: Option<String>,
    pub logo_preview_url_expires_at: Option<String>,
    pub country_code: String,
    pub event_starts_on: String,
    pub event_ends_on: String,
    pub description_markdown: String,
    pub currency_code: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ListPublicConventionsQuery {
    pub today: String,
    pub locale_country: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PublicConventionListResponse {
    pub items: Vec<PublicConvention>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
    pub visitor_country: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PublicCatalogueResponse {
    pub convention: PublicConvention,
    pub products: Vec<PublicProduct>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PublicProduct {
    pub id: String,
    pub title: String,
    pub description: String,
    pub product_type: String,
    pub featured_image: Option<CatalogueImage>,
    pub variants: Vec<PublicVariant>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PublicVariant {
    pub id: String,
    pub title: String,
    pub sku: Option<String>,
    pub image: Option<CatalogueImage>,
    pub selected_options: Vec<CatalogueOption>,
    pub final_price: String,
    pub inventory_policy: CatalogueInventoryPolicy,
    pub available_quantity: i64,
    pub max_quantity: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QuoteConventionRequest {
    pub items: Vec<QuoteConventionItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct QuoteConventionItem {
    pub variant_id: String,
    pub quantity: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConventionQuote {
    pub currency_code: String,
    pub items: Vec<ConventionQuoteLine>,
    pub subtotal: String,
    pub shipping: String,
    pub total: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConventionQuoteLine {
    pub variant_id: String,
    pub quantity: u32,
    pub unit_price: String,
    pub line_total: String,
}
