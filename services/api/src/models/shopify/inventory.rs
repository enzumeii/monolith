use super::{InventoryActivationCandidate, ShopifyId};
use crate::client::{ClientError, ClientErrorKind};

pub(crate) struct InventoryActivationItem {
    pub inventory_item_id: ShopifyId,
    pub product_id: u64,
}

impl InventoryActivationCandidate {
    pub(crate) fn activation_item(&self) -> Result<Option<InventoryActivationItem>, ClientError> {
        if self.is_active || !self.item.tracked {
            return Ok(None);
        }
        let on_hand = self.quantity("on_hand")?;
        if on_hand <= 0 {
            return Ok(None);
        }
        self.item.id.numeric_id("InventoryItem")?;
        self.item.variant.id.numeric_id("ProductVariant")?;
        let product_id = self.item.variant.product.id.numeric_id("Product")?;
        Ok(Some(InventoryActivationItem {
            inventory_item_id: self.item.id.clone(),
            product_id,
        }))
    }

    fn quantity(&self, name: &str) -> Result<i64, ClientError> {
        let mut quantities = self
            .quantities
            .iter()
            .filter(|quantity| quantity.name == name);
        let invalid = || ClientError::new(ClientErrorKind::Protocol("invalid activation quantity"));
        let quantity = quantities.next().ok_or_else(invalid)?;
        if quantities.next().is_some() {
            return Err(invalid());
        }
        Ok(quantity.quantity)
    }
}

impl ShopifyId {
    fn numeric_id(&self, kind: &str) -> Result<u64, ClientError> {
        self.require_kind(kind)?;
        self.0
            .rsplit('/')
            .next()
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::Protocol("invalid numeric Shopify ID"))
            })
    }
}
