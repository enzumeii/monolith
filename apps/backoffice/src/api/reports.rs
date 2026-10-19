use super::ApiResult;
use domain::dto::{InventoryQuery, InventoryResponse, OrderReportResponse};
use reqwest::RequestBuilder;
pub async fn get_inventory_report(id: &str, query: InventoryQuery) -> ApiResult<InventoryResponse> {
    super::decode(inventory_request(id, query)?).await
}
pub async fn get_picklist_report(tags: &str) -> ApiResult<OrderReportResponse> {
    super::decode(picklist_request(tags)?).await
}
fn inventory_request(id: &str, mut query: InventoryQuery) -> ApiResult<RequestBuilder> {
    if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) || id.bytes().all(|c| c == b'0') {
        return Err(super::ApiError::new(
            reqwest::StatusCode::BAD_REQUEST,
            "Enter a positive numeric inventory ID",
        ));
    }
    query.page_size = Some(query.page_size.unwrap_or(50));
    super::request(reqwest::Method::GET, &format!("/admin/inventories/{id}"))
        .map(|request| request.query(&query))
}
fn picklist_request(tags: &str) -> ApiResult<RequestBuilder> {
    if tags.split(',').any(|tag| tag.trim().is_empty()) {
        return Err(super::ApiError::new(
            reqwest::StatusCode::BAD_REQUEST,
            "Enter nonblank comma-separated tags",
        ));
    }
    super::request(reqwest::Method::GET, "/admin/orders/report")
        .map(|request| request.query(&domain::dto::OrderReportQuery { tags: tags.into() }))
}
#[cfg(test)]
#[path = "../tests/api/reports.rs"]
mod tests;
