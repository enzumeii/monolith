use crate::{error::AppError, services, state::AppState};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header::LOCATION},
};
use domain::dto::{
    CreateCronConfiguration, ListCronConfigurationsQuery, PatchCronConfiguration,
    PreviewCronConfiguration,
};
use std::str::FromStr;
use uuid::Uuid;

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListCronConfigurationsQuery>, QueryRejection>,
) -> Result<Json<domain::dto::CronConfigurationListResponse>, AppError> {
    let Query(query) = query.map_err(|_| bad_request("Invalid cron configuration query"))?;
    Ok(Json(
        services::cron_configurations::list(&state.pool, query).await?,
    ))
}

pub async fn create(
    State(state): State<AppState>,
    payload: Result<Json<CreateCronConfiguration>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        [(axum::http::HeaderName, HeaderValue); 1],
        Json<domain::dto::CronConfigurationDTO>,
    ),
    AppError,
> {
    let Json(payload) = payload.map_err(|_| bad_request("Invalid cron configuration body"))?;
    let response = services::cron_configurations::create(&state.pool, payload).await?;
    let location = HeaderValue::from_str(&format!("/v1/admin/cron-configurations/{}", response.id))
        .map_err(|_| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"))?;
    Ok((StatusCode::CREATED, [(LOCATION, location)], Json(response)))
}

pub async fn preview(
    payload: Result<Json<PreviewCronConfiguration>, JsonRejection>,
) -> Result<Json<domain::dto::CronConfigurationPreview>, AppError> {
    let Json(payload) = payload.map_err(|_| bad_request("Invalid cron configuration body"))?;
    Ok(Json(services::cron_configurations::preview(&payload)?))
}

pub async fn get(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    let id = configuration_id(path)?;
    Ok(Json(
        services::cron_configurations::get(&state.pool, id).await?,
    ))
}

pub async fn update(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    payload: Result<Json<PatchCronConfiguration>, JsonRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    let id = configuration_id(path)?;
    let Json(payload) =
        payload.map_err(|_| bad_request("Invalid cron configuration PATCH body"))?;
    Ok(Json(
        services::cron_configurations::update(&state.pool, id, payload).await?,
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<StatusCode, AppError> {
    let id = configuration_id(path)?;
    services::cron_configurations::delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn configuration_id(path: Result<Path<String>, PathRejection>) -> Result<Uuid, AppError> {
    let Path(id) = path.map_err(|_| bad_request("Invalid cron configuration ID"))?;
    Uuid::from_str(&id).map_err(|_| bad_request("Invalid cron configuration ID"))
}

fn bad_request(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
