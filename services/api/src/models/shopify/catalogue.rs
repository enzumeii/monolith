use super::{Connection, InventoryLevel, Money, ShopifyId};
use crate::client::{ClientError, ClientErrorKind};
use domain::dto::{CatalogueImage, CatalogueInventoryPolicy, CatalogueOption};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
pub(crate) struct CatalogueData {
    #[serde(deserialize_with = "complete_metadata")]
    pub products: Connection<CatalogueProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogueProduct {
    pub id: ShopifyId,
    pub title: String,
    pub description: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub featured_image: Option<CatalogueImage>,
    #[serde(deserialize_with = "complete_metadata")]
    pub variants: Connection<SelectionVariant>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectionVariant {
    pub id: ShopifyId,
    pub title: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub sku: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub image: Option<CatalogueImage>,
    pub selected_options: Vec<CatalogueOption>,
    pub inventory_policy: CatalogueInventoryPolicy,
    pub contextual_pricing: super::Pricing,
    pub inventory_item: SelectionInventory,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectionInventory {
    pub id: ShopifyId,
    pub tracked: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pub inventory_level: Option<InventoryLevel>,
}

impl SelectionVariant {
    pub(crate) fn available_quantity(&self) -> Result<Option<i64>, ClientError> {
        self.id.number("ProductVariant")?;
        self.inventory_item.id.number("InventoryItem")?;
        let Money {
            amount,
            currency_code,
        } = &self.contextual_pricing.price;
        crate::utils::numbers::parse_f64(amount)?;
        if currency_code.len() != 3 || !currency_code.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(invalid("catalogue currency is missing or invalid"));
        }
        let Some(level) = &self.inventory_item.inventory_level else {
            return Ok(None);
        };
        if level.id.trim().is_empty() {
            return Err(invalid("inventory level ID is missing"));
        }
        if !level.is_active {
            return Ok(None);
        }
        let mut quantities = level
            .quantities
            .iter()
            .filter(|quantity| quantity.name == "available");
        let quantity = quantities
            .next()
            .ok_or_else(|| invalid("available quantity is missing"))?;
        if quantities.next().is_some() {
            return Err(invalid("available quantity is duplicated"));
        }
        Ok(Some(quantity.quantity))
    }
}

fn complete_metadata<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Connection<T>, D::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RequiredPageInfo {
        has_next_page: bool,
        #[serde(deserialize_with = "Option::deserialize")]
        end_cursor: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RequiredConnection<T> {
        nodes: Vec<T>,
        page_info: RequiredPageInfo,
    }
    let wire = RequiredConnection::<T>::deserialize(deserializer)?;
    Ok(Connection {
        nodes: wire.nodes,
        page_info: super::PageInfo {
            has_next_page: wire.page_info.has_next_page,
            end_cursor: wire.page_info.end_cursor,
        },
    })
}

fn invalid(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(message))
}

#[cfg(test)]
#[path = "../../tests/models/shopify/catalogue.rs"]
mod tests;
