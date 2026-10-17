use crate::{
    error::AppError, services::products::product_report, state::AppState,
    validations::products::extract_report_query,
};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{ProductReportQuery, ProductsResponse};

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ProductReportQuery>, QueryRejection>,
) -> Result<Json<ProductsResponse>, AppError> {
    let input = extract_report_query(query)?;
    Ok(Json(product_report(&state, input).await?))
}
