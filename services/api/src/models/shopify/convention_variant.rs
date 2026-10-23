use super::{ProductStatus, ShopifyId};
use domain::dto::CatalogueImage;
use serde::{Deserialize, Deserializer};
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
    #[serde(deserialize_with = "image")]
    pub image: Option<CatalogueImage>,
    pub inventory_policy: domain::dto::CatalogueInventoryPolicy,
    pub inventory_item: Inventory,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Product {
    pub title: String,
    #[serde(deserialize_with = "image")]
    pub featured_image: Option<CatalogueImage>,
    pub id: ShopifyId,
    pub status: ProductStatus,
}
#[derive(Deserialize)]
pub(crate) struct Inventory {
    pub tracked: bool,
}

fn image<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<CatalogueImage>, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Image {
        url: String,
    }
    Ok(
        Option::<Image>::deserialize(deserializer)?.map(|image| CatalogueImage {
            url: image.url,
            alt_text: None,
        }),
    )
}

#[cfg(test)]
#[path = "../../tests/models/shopify/convention_variant.rs"]
mod tests;
