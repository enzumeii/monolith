use super::{ProductStatus, ShopifyId};
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
    pub inventory_policy: domain::dto::CatalogueInventoryPolicy,
    pub inventory_item: Inventory,
}
#[derive(Deserialize)]
pub(crate) struct Product {
    pub id: ShopifyId,
    pub status: ProductStatus,
}
#[derive(Deserialize)]
pub(crate) struct Inventory {
    pub tracked: bool,
}
