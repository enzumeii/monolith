use crate::{error::AppError, services::cron::report, state::AppState, validations};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
};
use domain::dto::ListReportResultsQuery;

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListReportResultsQuery>, QueryRejection>,
) -> Result<Json<report::ReportPage>, AppError> {
    let query = validations::cron::query(query)?;
    Ok(Json(
        report::list(&state.pool, validations::report_results::filter(&query)?).await?,
    ))
}
pub async fn detail(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::ReportResultDTO>, AppError> {
    Ok(Json(
        report::detail(&state.pool, validations::cron::id(path)?).await?,
    ))
}
