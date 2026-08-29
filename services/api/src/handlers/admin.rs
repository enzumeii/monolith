use axum::{Json, extract::State};
use domain::dto::{AdminLoginRequest, AdminLoginResponse};
use tracing::instrument;

use crate::{error::AppError, services::auth::AuthService, state::AppState};

pub struct AdminHandler;

impl AdminHandler {
    #[instrument(skip(state, payload), fields(username = %payload.username))]
    pub async fn login_handler(
        State(state): State<AppState>,
        Json(payload): Json<AdminLoginRequest>,
    ) -> Result<Json<AdminLoginResponse>, AppError> {
        let response =
            AuthService::login(&state.db, state.settings.jwt.secret.as_bytes(), payload).await?;

        Ok(Json(response))
    }
}
