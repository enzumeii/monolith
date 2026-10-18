use super::ProductReportRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderReportQuery {
    pub tags: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OrderReportResponse {
    pub products: Vec<ProductReportRow>,
}
