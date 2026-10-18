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
        services::products::inventory_report(&state, location, page).await?,
    ))
}
