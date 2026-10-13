use crate::auth::Claims;
use crate::{error::AppError, services, state::AppState, validations::admin as validate};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use domain::dto::{
    AdminDTO, AdminListResponse, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListAdminsQuery>,
) -> Result<Json<AdminListResponse>, AppError> {
    let page = validate::validate_page(&query)?;
    let response = services::admin::list(&state.pool, page).await?;
    Ok(Json(response))
}

pub async fn add(
    State(state): State<AppState>,
    Json(payload): Json<CreateAdminRequest>,
) -> Result<(StatusCode, Json<AdminDTO>), AppError> {
    let response = services::admin::add(&state.pool, payload).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateAdminRequest>,
) -> Result<Json<AdminDTO>, AppError> {
    validate::validate_update(&claims, id)?;

    let response = services::admin::update(&state.pool, id, payload).await?;
    services::auth::logout(&state.pool, id).await?;

    Ok(Json(response))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    let response = services::admin::get(&state.pool, id).await?;

    Ok(Json(response))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    validate::validate_delete(&claims, id)?;

    let response = services::admin::delete(&state.pool, id).await?;
    services::auth::logout(&state.pool, id).await?;

    Ok(Json(response))
}
