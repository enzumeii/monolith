use super::dependencies::shopify_client;
use crate::{error::AppError, services, state::AppState, validations};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
};
use domain::dto::{InventoryQuery, InventoryResponse};

pub async fn get(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<InventoryQuery>, QueryRejection>,
) -> Result<Json<InventoryResponse>, AppError> {
    let (location, page) = validations::inventories::extract_inventory_request(path, query)?;
    Ok(Json(
        services::inventories::inventory_report(shopify_client(&state)?, location, page).await?,
    ))
}

pub async fn post_enable(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
) -> Result<Json<domain::dto::InventoryEnableResponse>, AppError> {
    let location = validations::inventories::extract_enable_request(path, query.as_deref())?;
    Ok(Json(
        services::inventories::enable_inventory(shopify_client(&state)?, location).await?,
    ))
}
