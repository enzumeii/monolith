use super::{ApiError, ApiResult};
use domain::dto::InventoryEnableResponse;
use reqwest::{Method, RequestBuilder, StatusCode};

pub async fn enable_inventory(location_id: &str) -> ApiResult<InventoryEnableResponse> {
    super::decode(enable_request(location_id)?).await
}

fn enable_request(location_id: &str) -> ApiResult<RequestBuilder> {
    if !super::valid_location_id(location_id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Enter a positive numeric location ID",
        ));
    }
    super::request(
        Method::POST,
        &format!("/admin/inventories/{location_id}/enable"),
    )
}

#[cfg(test)]
#[path = "../tests/api/inventories.rs"]
mod tests;
