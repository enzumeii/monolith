use super::dependencies::shopify_client;
use crate::{error::AppError, services, state::AppState, validations};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{OrderReportQuery, OrderReportResponse};

pub async fn report(
    State(state): State<AppState>,
    query: Result<Query<OrderReportQuery>, QueryRejection>,
) -> Result<Json<OrderReportResponse>, AppError> {
    let tags = validations::orders::extract_order_query(query)?;
    Ok(Json(
        services::orders::order_report(shopify_client(&state)?, tags).await?,
    ))
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<domain::dto::orders::ListOrdersQuery>, QueryRejection>,
) -> Result<Json<domain::dto::orders::ListOrdersResponse>, AppError> {
    let (search, page) = validations::orders::extract_list_query(query)?;
    Ok(Json(
        services::orders::list(shopify_client(&state)?, search, page).await?,
    ))
}
pub async fn shipping_labels(
    State(state): State<AppState>,
) -> Result<
    (
        [(axum::http::header::HeaderName, &'static str); 1],
        Json<domain::dto::orders::ShippingLabels>,
    ),
    AppError,
> {
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(
            services::orders::shipping_label::labels(
                shopify_client(&state)?,
                state.settings.shipping_sender.as_ref(),
            )
            .await?,
        ),
    ))
}
pub async fn fulfill(
    State(state): State<AppState>,
    path: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
    body: Result<
        Json<domain::dto::orders::FulfillOrderRequest>,
        axum::extract::rejection::JsonRejection,
    >,
) -> Result<Json<domain::dto::orders::FulfillOrderResponse>, AppError> {
    let (id, body) = validations::orders::extract_fulfill_request(path, body)?;
    Ok(Json(
        services::orders::fulfillment::fulfill(shopify_client(&state)?, id, body).await?,
    ))
}

pub async fn reship(
    State(state): State<AppState>,
    uri: axum::http::Uri,
    path: Result<axum::extract::Path<String>, axum::extract::rejection::PathRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<domain::dto::orders::ReshipOrderResponse>, AppError> {
    let id = validations::orders::extract_reship_request(path, uri.query(), body)?;
    Ok(Json(
        services::orders::reship::reship(
            shopify_client(&state)?,
            super::dependencies::resend_client(&state)?,
            id,
        )
        .await?,
    ))
}
