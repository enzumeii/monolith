use crate::{error::AppError, state::AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use domain::dto::{AdminDTO, AdminListResponse, Claims, CreateAdminRequest, UpdateAdminRequest};

pub struct AdminHandler;

impl AdminHandler {
    pub async fn list(State(state): State<AppState>) -> Result<Json<AdminListResponse>, AppError> {
        let response = state.admin_service.list().await?;
        Ok(Json(response))
    }

    pub async fn add(
        State(state): State<AppState>,
        Json(payload): Json<CreateAdminRequest>,
    ) -> Result<(StatusCode, Json<AdminDTO>), AppError> {
        let response = state.admin_service.add(payload).await?;

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

        let response = state.admin_service.update(id, payload).await?;
        state.auth_service.logout(claims.admin_id).await?;

        Ok(Json(response))
    }

    pub async fn get(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<Json<AdminDTO>, AppError> {
        let response = state.admin_service.get(id).await?;

        Ok(Json(response))
    }

    pub async fn delete(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<Json<AdminDTO>, AppError> {
        let response = state.admin_service.delete(id).await?;
        state.auth_service.logout(id).await?;

        Ok(Json(response))
    }
}
