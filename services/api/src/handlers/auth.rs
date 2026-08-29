use axum::{Extension, Json, extract::State, http::StatusCode};
use domain::dto::{Claims, LoginRequest, RefreshRequest, TokenResponse};
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

    pub async fn logout(
        State(state): State<AppState>,
        Extension(claims): Extension<Claims>,
    ) -> Result<StatusCode, AppError> {
        AuthService::logout(&state.db, claims.admin_id).await?;

        Ok(StatusCode::NO_CONTENT)
    }
}
