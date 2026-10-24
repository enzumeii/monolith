use super::{ProductStatus, ShopifyId, convention_variant::Inventory};
use domain::dto::{CatalogueImage, CatalogueInventoryPolicy, CatalogueOption};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct VariantNodesData {
    pub nodes: Vec<Option<VariantNode>>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
pub(crate) enum VariantNode {
    ProductVariant(LiveVariant),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveVariant {
    pub id: ShopifyId,
    pub title: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub sku: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub image: Option<CatalogueImage>,
    pub selected_options: Vec<CatalogueOption>,
    pub inventory_policy: CatalogueInventoryPolicy,
    #[serde(deserialize_with = "Option::deserialize")]
    pub inventory_quantity: Option<i64>,
    pub inventory_item: Inventory,
    pub product: LiveProduct,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveProduct {
    pub id: ShopifyId,
    pub title: String,
    pub description: String,
    pub product_type: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub featured_image: Option<CatalogueImage>,
    pub status: ProductStatus,
}

impl LiveVariant {
    pub(crate) fn available_quantity(&self) -> Result<i64, crate::client::ClientError> {
        self.inventory_quantity.ok_or_else(|| {
            crate::client::ClientError::new(crate::client::ClientErrorKind::Protocol(
                "Tracked variant inventory quantity is missing",
            ))
        })
    }
}
