use crate::{
    error::AppError, services::cron::config as cron_config, state::AppState,
    validations::cron::configs as validate,
};
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

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListCronConfigurationsQuery>, QueryRejection>,
) -> Result<Json<domain::dto::CronConfigurationListResponse>, AppError> {
    let query = validate::validate_query(query)?;
    Ok(Json(
        cron_config::list(&state.pool, validate::validate_filter(&query)?).await?,
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
    let input = validate::validate_create(validate::validate_body(payload)?)?;
    let result = cron_config::create(&state.pool, &input).await?;
    let location = HeaderValue::from_str(&format!("/v1/admin/cron/configs/{}", result.id))
        .map_err(anyhow::Error::new)?;
    Ok((StatusCode::CREATED, [(LOCATION, location)], Json(result)))
}
pub async fn preview(
    query: Result<Query<PreviewCronConfiguration>, QueryRejection>,
) -> Result<Json<domain::dto::CronConfigurationPreview>, AppError> {
    let request = validate::validate_preview(&validate::validate_query(query)?)?;
    Ok(Json(cron_config::preview(&request)?))
}
pub async fn get(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    Ok(Json(
        cron_config::get(&state.pool, validate::validate_id(path)?)
            .await?
            .into_dto()?,
    ))
}
pub async fn update(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    payload: Result<Json<PatchCronConfiguration>, JsonRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    let id = validate::validate_id(path)?;
    let changes = validate::validate_body(payload)?;
    validate::validate_patch_shape(&changes)?;
    let current = cron_config::get(&state.pool, id).await?;
    let input = validate::validate_patch(changes, &current)?;
    Ok(Json(
        cron_config::update(&state.pool, id, current.updated_at, &input).await?,
    ))
}
pub async fn delete(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<StatusCode, AppError> {
    cron_config::delete(&state.pool, validate::validate_id(path)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
