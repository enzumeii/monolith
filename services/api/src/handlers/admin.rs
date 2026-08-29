use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use domain::dto::{AdminDTO, AdminListResponse, Claims, CreateAdminRequest, UpdateAdminRequest};

use crate::{
    error::AppError,
    services::{admin::AdminService, auth::AuthService},
    state::AppState,
};

pub struct AdminHandler;

impl AdminHandler {
    pub async fn list(State(state): State<AppState>) -> Result<Json<AdminListResponse>, AppError> {
        let response = AdminService::list(&state.db).await?;
        Ok(Json(response))
    }

    pub async fn add(
        State(state): State<AppState>,
        Json(payload): Json<CreateAdminRequest>,
    ) -> Result<(StatusCode, Json<AdminDTO>), AppError> {
        let response = AdminService::add(&state.db, payload).await?;

        Ok((StatusCode::CREATED, Json(response)))
    }

    pub async fn update(
        State(state): State<AppState>,
        Extension(claims): Extension<Claims>,
        Path(id): Path<i64>,
        Json(payload): Json<UpdateAdminRequest>,
    ) -> Result<Json<AdminDTO>, AppError> {
        if claims.admin_id != id {
            return Err(AppError::Forbidden);
        }

        let response = AdminService::update(&state.db, id, payload).await?;
        AuthService::logout(&state.db, claims.admin_id).await?;

        Ok(Json(response))
    }

    pub async fn get(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<Json<AdminDTO>, AppError> {
        let response = AdminService::get(&state.db, id).await?;

        Ok(Json(response))
    }

    pub async fn delete(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<Json<AdminDTO>, AppError> {
        let response = AdminService::delete(&state.db, id).await?;
        AuthService::logout(&state.db, id).await?;

        Ok(Json(response))
    }
}
