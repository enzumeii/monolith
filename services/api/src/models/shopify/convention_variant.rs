use super::{ProductStatus, ShopifyId};
use domain::dto::CatalogueImage;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VariantData {
    #[serde(deserialize_with = "Option::deserialize")]
    pub product_variant: Option<Variant>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Variant {
    pub id: ShopifyId,
    pub product: Product,
    pub title: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub sku: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub image: Option<CatalogueImage>,
    pub inventory_policy: domain::dto::CatalogueInventoryPolicy,
    pub inventory_item: Inventory,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Product {
    pub title: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub featured_image: Option<CatalogueImage>,
    pub id: ShopifyId,
    pub status: ProductStatus,
}
#[derive(Deserialize)]
pub(crate) struct Inventory {
    pub tracked: bool,
}

#[cfg(test)]
#[path = "../../tests/models/shopify/convention_variant.rs"]
mod tests;
