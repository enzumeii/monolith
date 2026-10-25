use super::{InventoryActivationCandidate, ShopifyId};
use crate::client::{ClientError, ClientErrorKind};

impl InventoryActivationCandidate {
    pub(crate) fn quantity(&self, name: &str) -> Result<i64, ClientError> {
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
    pub(crate) fn numeric_id(&self, kind: &str) -> Result<u64, ClientError> {
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
