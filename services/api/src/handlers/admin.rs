use crate::{error::AppError, services, state::AppState};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use domain::dto::{
    AdminDTO, AdminListResponse, Claims, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListAdminsQuery>,
) -> Result<Json<AdminListResponse>, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    let page = services::admin::validate_page(page, per_page)?;

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
    services::admin::ensure_self(claims.admin_id, id)?;

    let response = services::admin::update(&state.pool, id, payload).await?;
    services::auth::logout(&state.pool, claims.admin_id).await?;

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
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    let response = services::admin::delete(&state.pool, id).await?;
    services::auth::logout(&state.pool, id).await?;

    Ok(Json(response))
}
