use crate::{error::AppError, services, state::AppState};
use axum::{
    Json,
    body::Body,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderValue, Response, StatusCode, header},
};
use domain::dto::{ListReportResultsQuery, ReportRowsQuery};
use serde::Deserialize;
use std::str::FromStr;
use uuid::Uuid;

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListReportResultsQuery>, QueryRejection>,
) -> Result<Json<services::report_results::ReportPage>, AppError> {
    let Query(query) = query.map_err(|_| bad_request("Invalid report result query"))?;
    Ok(Json(
        services::report_results::list(&state.pool, query).await?,
    ))
}

pub async fn detail(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::ReportResultDetail>, AppError> {
    Ok(Json(
        services::report_results::detail(&state.pool, result_id(path)?).await?,
    ))
}

pub async fn rows(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ReportRowsQuery>, QueryRejection>,
) -> Result<Json<domain::dto::ReportRowsResponse>, AppError> {
    let id = result_id(path)?;
    let Query(query) = query.map_err(|_| bad_request("Invalid report rows query"))?;
    Ok(Json(
        services::report_results::rows(
            &state.pool,
            id,
            &query.dataset,
            query.page.unwrap_or(1),
            query.per_page.unwrap_or(25),
        )
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    dataset: String,
}

pub async fn download(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<DownloadQuery>, QueryRejection>,
) -> Result<Response<Body>, AppError> {
    let id = result_id(path)?;
    let Query(query) = query.map_err(|_| bad_request("Invalid report download query"))?;
    let (csv, filename) = services::report_results::csv(&state.pool, id, &query.dataset).await?;
    let mut response = Response::new(Body::from(csv));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| bad_request("Invalid report filename"))?,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn result_id(path: Result<Path<String>, PathRejection>) -> Result<Uuid, AppError> {
    let Path(id) = path.map_err(|_| bad_request("Invalid report result ID"))?;
    Uuid::from_str(&id).map_err(|_| bad_request("Invalid report result ID"))
}

fn bad_request(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
