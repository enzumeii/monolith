use crate::models::{AuthSource, AuthenticatedAdmin};
use crate::{error::AppError, services, state::AppState, validations};
use axum::response::{IntoResponse, Response};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use axum_extra::extract::cookie::CookieJar;
use domain::dto::{
    AdminDTO, AdminListResponse, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListAdminsQuery>,
) -> Result<Json<AdminListResponse>, AppError> {
    let page = validations::admin::validate_page(&query)?;
    let response = services::admin::list(&state.pool, page).await?;
    Ok(Json(response))
}

pub async fn add(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedAdmin>,
    Json(payload): Json<CreateAdminRequest>,
) -> Result<(StatusCode, Json<AdminDTO>), AppError> {
    validations::admin::validate_create(&payload)?;
    let response = services::admin::add(&state.pool, &actor, payload).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedAdmin>,
    Extension(source): Extension<AuthSource>,
    Path(id): Path<i64>,
    jar: CookieJar,
    Json(payload): Json<UpdateAdminRequest>,
) -> Result<Response, AppError> {
    services::admin::authorize_update(&actor, id)?;
    validations::admin::validate_update_input(&payload)?;
    let response = services::admin::update(&state.pool, &actor, id, payload).await?;
    if source == AuthSource::Cookie && actor.id == id {
        Ok((
            crate::middleware::cookies::clear_cookie(jar, &state.settings.browser),
            Json(response),
        )
            .into_response())
    } else {
        Ok(Json(response).into_response())
    }
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
    Extension(claims): Extension<AuthenticatedAdmin>,
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    let response = services::admin::delete(&state.pool, &claims, id).await?;

    Ok(Json(response))
}
