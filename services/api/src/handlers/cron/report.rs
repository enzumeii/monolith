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
    let query = validations::cron::configs::validate_query(query)?;
    Ok(Json(
        report::list(
            &state.pool,
            validations::cron::report::validate_filter(&query)?,
        )
        .await?,
    ))
}
pub async fn detail(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::ReportResultDTO>, AppError> {
    Ok(Json(
        report::detail(&state.pool, validations::cron::configs::validate_id(path)?).await?,
    ))
}
