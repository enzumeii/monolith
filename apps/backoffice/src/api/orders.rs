use super::{ApiError, ApiResult};
use domain::dto::orders::{
    Courier, FulfillOrderRequest, FulfillOrderResponse, ListOrdersResponse, ReshipOrderResponse,
    ShippingLabels,
};
use reqwest::{Method, RequestBuilder, StatusCode};

pub async fn list_orders(tag: &str, cursor: Option<String>) -> ApiResult<ListOrdersResponse> {
    super::decode(list_request(tag, cursor)?).await
}

pub async fn shipping_labels() -> ApiResult<ShippingLabels> {
    super::decode(shipping_labels_request()?).await
}

pub async fn fulfill(
    order_id: u64,
    courier: Courier,
    tracking_number: String,
) -> ApiResult<FulfillOrderResponse> {
    super::decode(fulfill_request(order_id, courier, tracking_number)?).await
}

pub async fn reship(order_number: &str) -> ApiResult<ReshipOrderResponse> {
    super::decode(reship_request(order_number)?).await
}

fn list_request(tag: &str, cursor: Option<String>) -> ApiResult<RequestBuilder> {
    if tag.split(',').any(|part| part.trim().is_empty()) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "Enter order tags"));
    }
    let request = super::request(Method::GET, "/admin/orders")?.query(&[
        ("tag", tag),
        ("fulfillment", "UNFULFILLED"),
        ("page_size", "250"),
    ]);
    Ok(match cursor {
        Some(cursor) => request.query(&[("cursor", cursor)]),
        None => request,
    })
}

fn shipping_labels_request() -> ApiResult<RequestBuilder> {
    super::request(Method::POST, "/admin/orders/shipping_label")
}

fn fulfill_request(
    order_id: u64,
    courier: Courier,
    tracking_number: String,
) -> ApiResult<RequestBuilder> {
    if order_id == 0 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Enter a valid order ID",
        ));
    }
    Ok(
        super::request(Method::POST, &format!("/admin/orders/{order_id}/fulfill"))?.json(
            &FulfillOrderRequest {
                courier,
                tracking_number,
            },
        ),
    )
}

fn reship_request(order_number: &str) -> ApiResult<RequestBuilder> {
    if !valid_order_number(order_number) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Enter a four-digit order number between 1000 and 9999",
        ));
    }
    super::request(
        Method::POST,
        &format!("/admin/orders/{order_number}/reship"),
    )
}

pub(crate) fn valid_order_number(order_number: &str) -> bool {
    order_number.len() == 4
        && order_number.bytes().all(|byte| byte.is_ascii_digit())
        && order_number
            .parse::<u16>()
            .is_ok_and(|number| (1000..=9999).contains(&number))
}

#[cfg(test)]
#[path = "../tests/api/orders.rs"]
mod tests;
