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
    let tag = validations::orders::extract_order_query(query)?;
    Ok(Json(services::products::order_report(&state, tag).await?))
}
