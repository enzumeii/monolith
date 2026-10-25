use crate::{error::AppError, services, state::AppState, transformers, validations};
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
) -> Result<Json<services::cron::report::ReportPage>, AppError> {
    let query = crate::validations::common::query(query, "Invalid cron query")?;
    Ok(Json(
        services::cron::report::list(
            &state.pool,
            transformers::cron::report::filter(&validations::cron::report::validate_filter(
                &query,
            )?),
        )
        .await?,
    ))
}
pub async fn detail(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::ReportResultDTO>, AppError> {
    Ok(Json(
        services::cron::report::detail(
            &state.pool,
            crate::validations::common::uuid_path(path, "Invalid cron identifier")?,
        )
        .await?,
    ))
}

pub async fn download_url(
    State(state): State<AppState>,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (id, filename) = validations::cron::report::validate_download_path(path)?;
    let client = crate::state::required_client(&state.s3, "S3 client unavailable")?;
    let ttl_secs = state
        .settings
        .client
        .as_ref()
        .and_then(|clients| clients.s3.as_ref())
        .ok_or_else(|| {
            AppError::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "S3 client unavailable",
            )
        })?
        .signed_url_ttl_secs;
    let result =
        services::cron::report::download_url(&state.pool, client, id, &filename, ttl_secs).await?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(result),
    ))
}
