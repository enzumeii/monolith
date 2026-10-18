use super::ProductReportRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryQuery {
    pub page_size: Option<u16>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InventoryPageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InventoryResponse {
    pub products: Vec<ProductReportRow>,
    pub page_info: InventoryPageInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InventoryEnableResponse {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<InventoryEnableError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InventoryEnableError {
    pub product_id: u64,
    pub reason: String,
}
