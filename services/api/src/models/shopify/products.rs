use super::InventoryLevel;
use crate::client::{ClientError, ClientErrorKind};
impl InventoryLevel {
    pub(crate) fn on_hand(&self) -> Result<i64, ClientError> {
        self.quantities
            .iter()
            .find(|q| q.name == "on_hand")
            .map(|q| q.quantity)
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::Protocol("on_hand quantity is missing"))
            })
    }
}

#[cfg(test)]
#[path = "../../tests/models/shopify/products.rs"]
mod tests;
