use crate::auth::Claims;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_auth::AuthBearer;
use domain::dto::AdminRole;
use jsonwebtoken::{DecodingKey, Validation, decode};

use crate::{error::AppError, state::AppState};

pub async fn auth_middleware(
    State(state): State<AppState>,
    AuthBearer(token): AuthBearer,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let validation = Validation {
        validate_exp: true,
        leeway: 60,
        ..Default::default()
    };

    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.settings.jwt.secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::InvalidToken)?;

    req.extensions_mut().insert(token_data.claims);

    Ok(next.run(req).await)
}

pub async fn require_super_admin(req: Request, next: Next) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<Claims>()
        .ok_or(AppError::InvalidCredentials)?;

    if claims.role != AdminRole::SuperAdmin {
        return Err(AppError::Forbidden);
    }

    Ok(next.run(req).await)
}
