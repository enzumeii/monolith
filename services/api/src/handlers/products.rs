use crate::{error::AppError, state::AppState};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{ProductReportQuery, ProductsResponse};

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ProductReportQuery>, QueryRejection>,
) -> Result<Json<ProductsResponse>, AppError> {
    let input = validations::products::extract_report_query(query)?;
    Ok(Json(
        services::products::product_report(&state, input).await?,
    ))
}
