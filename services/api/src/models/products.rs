use crate::models::{
    Sku,
    shopify::{InventoryMetadata, Money},
};

pub(crate) struct ReportInput {
    pub title: String,
    pub variant: Option<String>,
    pub sku: Option<Sku>,
    pub category: Option<String>,
    pub price: Money,
    pub quantity: i64,
    pub metadata: Option<InventoryMetadata>,
}
