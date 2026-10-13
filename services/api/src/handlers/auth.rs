use crate::auth::AuthenticatedAdmin;
use axum::{Extension, Json, extract::State, http::StatusCode};
use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};
use tracing::instrument;

use crate::{error::AppError, services, state::AppState};

#[instrument(skip(state, payload), fields(username = %payload.username))]
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let response = services::auth::login(&state.pool, state.jwt_key.as_slice(), payload).await?;

    Ok(Json(response))
}

#[instrument(skip(state, payload))]
pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let response = services::auth::refresh(&state.pool, state.jwt_key.as_slice(), payload).await?;

    Ok(Json(response))
}

pub async fn logout(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthenticatedAdmin>,
) -> Result<StatusCode, AppError> {
    services::auth::logout(&state.pool, &claims).await?;

    Ok(StatusCode::NO_CONTENT)
}
