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
    let client = state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::ReportUnavailable(Some(error.clone())).at("api::client::shopify")
        })?
        .as_ref()
        .ok_or_else(|| AppError::ReportUnavailable(None).at(module_path!()))?;
    Ok(Json(product_report(client, input).await?))
}
