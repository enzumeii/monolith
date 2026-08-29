use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest};

use crate::{error::AppError, services::admin::AdminService, state::AppState};

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

        Ok(Json(response))
    }
}
