use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_auth::AuthBearer;
use domain::dto::Claims;
use jsonwebtoken::{DecodingKey, decode};

use crate::{error::AppError, state::AppState};

pub async fn auth_middleware(
    State(state): State<AppState>,
    AuthBearer(token): AuthBearer,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let validation = jsonwebtoken::Validation {
        validate_exp: true,
        leeway: 60,
        ..Default::default()
    };

    // 2. Decode & validate
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.settings.jwt.secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::InvalidToken)?;
    req.extensions_mut().insert(token_data.claims);

    Ok(next.run(req).await)
}
