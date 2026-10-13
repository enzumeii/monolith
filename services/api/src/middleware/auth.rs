use crate::auth::{AuthenticatedAdmin, Claims};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use domain::dto::AdminRole;
use jsonwebtoken::{DecodingKey, Validation, decode};

use crate::{error::AppError, state::AppState};

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::InvalidToken)?;
    let validation = Validation {
        validate_exp: true,
        leeway: 60,
        ..Default::default()
    };
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_key.as_slice()),
        &validation,
    )
    .map_err(|_| AppError::InvalidToken)?;
    req.extensions_mut().insert(AuthenticatedAdmin {
        id: data.claims.admin_id,
        role: data.claims.role,
    });
    req.extensions_mut().insert(data.claims);

    Ok(next.run(req).await)
}

pub async fn require_super_admin(req: Request, next: Next) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<AuthenticatedAdmin>()
        .ok_or(AppError::InvalidCredentials)?;

    if claims.role != AdminRole::SuperAdmin {
        return Err(AppError::Forbidden);
    }

    Ok(next.run(req).await)
}
