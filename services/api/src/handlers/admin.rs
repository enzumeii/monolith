use axum::{Json, extract::State};
use domain::dto::AdminListResponse;

use crate::{error::AppError, services::admin::AdminService, state::AppState};

pub struct AdminHandler;

impl AdminHandler {
    pub async fn list_admins(
        State(state): State<AppState>,
    ) -> Result<Json<AdminListResponse>, AppError> {
        let response = AdminService::list_admins(&state.db).await?;
        Ok(Json(response))
    }
}
