use crate::error::AppError;
use axum::extract::{Query, rejection::QueryRejection};
use axum::http::StatusCode;
use domain::dto::OrderReportQuery;

pub fn extract_order_query(
    query: Result<Query<OrderReportQuery>, QueryRejection>,
) -> Result<Vec<String>, AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a nonblank tags value without pagination",
        )
    };
    let Query(query) = query.map_err(|_| invalid())?;
    let tags: Vec<String> = query
        .tags
        .split(',')
        .map(|tag| tag.trim().to_owned())
        .collect();
    if tags.iter().any(String::is_empty) {
        return Err(invalid());
    }
    Ok(tags)
}

pub fn extract_list_query(
    query: Result<Query<domain::dto::orders::ListOrdersQuery>, QueryRejection>,
) -> Result<(String, crate::models::shopify::PageRequest), AppError> {
    use domain::dto::orders::FulfillmentStatus;
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected tag, a valid fulfillment, page_size between 1 and 250, and a nonblank cursor",
        )
    };
    let Query(query) = query.map_err(|_| invalid())?;
    let request = query
        .tag
        .map(|tag| crate::models::shopify::OrdersByTagRequest {
            tags: tag.split(',').map(|tag| tag.trim().to_owned()).collect(),
        });
    let page = crate::models::shopify::PageRequest {
        first: query.page_size.unwrap_or(50),
        after: query.cursor,
    };
    page.validate().map_err(|_| invalid())?;
    let mut search = if let Some(request) = request {
        request.variables(&page).map_err(|_| invalid())?["search"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned()
    } else {
        String::new()
    };
    if let Some(status) = query.fulfillment {
        let status = match status {
            FulfillmentStatus::Unfulfilled => "unfulfilled",
            FulfillmentStatus::Fulfilled => "fulfilled",
            FulfillmentStatus::Partial => "partial",
            FulfillmentStatus::Unshipped => "unshipped",
            FulfillmentStatus::Shipped => "shipped",
            FulfillmentStatus::Any => "any",
        };
        if !search.is_empty() {
            search.push_str(" AND ");
        }
        search.push_str("fulfillment_status:");
        search.push_str(status);
    }
    Ok((search, page))
}

pub fn extract_fulfill_request(
    path: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
    body: Result<
        axum::Json<domain::dto::orders::FulfillOrderRequest>,
        axum::extract::rejection::JsonRejection,
    >,
) -> Result<(String, domain::dto::orders::FulfillOrderRequest), AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a positive order id, supported courier, and tracking_number of 1 to 256 characters without whitespace or control characters",
        )
    };
    let axum::extract::Path(id) = path.map_err(|_| invalid())?;
    let id = format!("gid://shopify/Order/{id}");
    crate::models::shopify::ShopifyId::new(id.clone())
        .and_then(|id| id.number("Order"))
        .map_err(|_| invalid())?;
    let axum::Json(body) = body.map_err(|_| invalid())?;
    if matches!(body.tracking_number.as_str(), "." | "..")
        || body.tracking_number.is_empty()
        || body.tracking_number.chars().count() > 256
        || body
            .tracking_number
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid());
    }
    Ok((id, body))
}

pub fn extract_reship_request(
    path: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
    query: Option<&str>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<u16, AppError> {
    let invalid = || {
        AppError::new(
            StatusCode::BAD_REQUEST,
            "Expected a four-digit order number (1000-9999) without query parameters or a request body",
        )
    };
    let axum::extract::Path(id) = path.map_err(|_| invalid())?;
    if query.is_some_and(|query| !query.is_empty())
        || !body.map_err(|_| invalid())?.is_empty()
        || id.len() != 4
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id
            .parse::<u16>()
            .ok()
            .is_none_or(|number| !(1000..=9999).contains(&number))
    {
        return Err(invalid());
    }
    id.parse().map_err(|_| invalid())
}

#[cfg(test)]
#[path = "../tests/validations/orders.rs"]
mod tests;
