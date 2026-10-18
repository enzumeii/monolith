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
    Ok(Json(services::orders::order_report(&state, tags).await?))
}
