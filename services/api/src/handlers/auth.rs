use axum::{Json, extract::State};
use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};
use tracing::instrument;

use crate::{error::AppError, services::auth::AuthService, state::AppState};

pub struct AuthHandler;

impl AuthHandler {
    #[instrument(skip(state, payload), fields(username = %payload.username))]
    pub async fn login(
        State(state): State<AppState>,
        Json(payload): Json<LoginRequest>,
    ) -> Result<Json<TokenResponse>, AppError> {
        let response =
            AuthService::login(&state.db, state.settings.jwt.secret.as_bytes(), payload).await?;

        Ok(Json(response))
    }

    #[instrument(skip(state, payload))]
    pub async fn refresh(
        State(state): State<AppState>,
        Json(payload): Json<RefreshRequest>,
    ) -> Result<Json<TokenResponse>, AppError> {
        let response =
            AuthService::refresh(&state.db, state.settings.jwt.secret.as_bytes(), payload).await?;

        Ok(Json(response))
    }
}
