use crate::{error::AppError, services, state::AppState, transformers, validations};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
};
use domain::dto::{
    CreateCronConfiguration, ListCronConfigurationsQuery, PatchCronConfiguration,
    PreviewCronConfiguration,
};

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListCronConfigurationsQuery>, QueryRejection>,
) -> Result<Json<domain::dto::CronConfigurationListResponse>, AppError> {
    let query = validations::common::query(query, "Invalid cron query")?;
    let (page, per_page) = validations::common::validate_page(query.page, query.per_page)?;
    let filter = transformers::cron::configs::filter(&query, page, per_page);
    Ok(Json(
        services::cron::config::list(&state.pool, filter).await?,
    ))
}
pub async fn create(
    State(state): State<AppState>,
    payload: Result<Json<CreateCronConfiguration>, JsonRejection>,
) -> Result<(StatusCode, Json<domain::dto::CronConfigurationDTO>), AppError> {
    let input = validations::cron::configs::validate_create(validations::common::body(
        payload,
        "Invalid cron request body",
    )?)?;
    let result = services::cron::config::create(&state.pool, input).await?;
    Ok((StatusCode::CREATED, Json(result)))
}
pub async fn preview(
    query: Result<Query<PreviewCronConfiguration>, QueryRejection>,
) -> Result<Json<domain::dto::CronConfigurationPreview>, AppError> {
    let request = validations::common::query(query, "Invalid cron query")?;
    Ok(Json(services::cron::config::preview(&request)?))
}
pub async fn get(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    Ok(Json(transformers::cron::configs::configuration(
        services::cron::config::get(
            &state.pool,
            validations::common::uuid_path(path, "Invalid cron identifier")?,
        )
        .await?,
    )?))
}
pub async fn update(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    payload: Result<Json<PatchCronConfiguration>, JsonRejection>,
) -> Result<Json<domain::dto::CronConfigurationDTO>, AppError> {
    let id = validations::common::uuid_path(path, "Invalid cron identifier")?;
    let changes = validations::common::body(payload, "Invalid cron request body")?;
    validations::cron::configs::validate_patch_shape(&changes)?;
    Ok(Json(
        services::cron::config::update(&state.pool, id, changes).await?,
    ))
}
pub async fn delete(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Result<StatusCode, AppError> {
    services::cron::config::delete(
        &state.pool,
        validations::common::uuid_path(path, "Invalid cron identifier")?,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
