use super::dependencies::shopify_client;
use crate::{error::AppError, services, state::AppState, validations};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{CatalogueQuery, CatalogueResponse};

pub async fn get(
    State(state): State<AppState>,
    query: Result<Query<CatalogueQuery>, QueryRejection>,
) -> Result<Json<CatalogueResponse>, AppError> {
    let (location, page, search) = validations::catalogue::extract_catalogue_request(query)?;
    Ok(Json(
        services::catalogue::products(shopify_client(&state)?, location, page, search.as_deref())
            .await?,
    ))
}
