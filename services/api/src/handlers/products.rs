use crate::{
    error::AppError, services::products::product_report, state::AppState,
    validations::products::validate_report_query,
};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{ProductReportQuery, ProductsResponse};

pub async fn get(
    State(state): State<AppState>,
    query: Result<Query<ProductReportQuery>, QueryRejection>,
) -> Result<Json<ProductsResponse>, AppError> {
    let Query(query) = query.map_err(|_| {
        AppError::BadRequest("Expected type INVENTORY or ORDER_TAG and an id".into())
            .at(module_path!())
    })?;
    let source = validate_report_query(query)?;
    Ok(Json(product_report(&state.settings, source).await?))
}
