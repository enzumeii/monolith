use super::cookies;
use crate::{
    config::BrowserConfig,
    error::AppError,
    models::{AuthenticatedAdmin, Claims},
};
use axum::http::StatusCode;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use domain::dto::AdminRole;
use jsonwebtoken::{DecodingKey, Validation, decode};

pub async fn auth_middleware(
    State((jwt_key, browser)): State<(std::sync::Arc<Vec<u8>>, BrowserConfig)>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let jar = CookieJar::from_headers(req.headers());
    let (token, source) = if let Some(token) = bearer(req.headers())? {
        (token, crate::models::AuthSource::Bearer)
    } else {
        (
            cookies::cookie_credentials(&jar, &browser)?.0,
            crate::models::AuthSource::Cookie,
        )
    };
    let claims = validate_token(token, jwt_key.as_slice())?;
    req.extensions_mut().insert(source);
    req.extensions_mut().insert(AuthenticatedAdmin {
        id: claims.admin_id,
        role: claims.role,
    });

    Ok(next.run(req).await)
}

pub async fn require_super_admin(req: Request, next: Next) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<AuthenticatedAdmin>()
        .ok_or_else(|| AppError::new(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    if claims.role != AdminRole::SuperAdmin {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action",
        ));
    }

    Ok(next.run(req).await)
}

pub(crate) fn bearer(headers: &axum::http::HeaderMap) -> Result<Option<&str>, AppError> {
    match headers.get("authorization") {
        None => Ok(None),
        Some(value) => value
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|token| !token.is_empty())
            .map(Some)
            .ok_or_else(|| {
                AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                    .with_bearer_challenge()
            }),
    }
}
pub(crate) fn validate_token(token: &str, key: &[u8]) -> Result<Claims, AppError> {
    let validation = Validation {
        validate_exp: true,
        leeway: 60,
        ..Default::default()
    };
    decode::<Claims>(token, &DecodingKey::from_secret(key), &validation)
        .map(|data| data.claims)
        .map_err(|error| {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .with_source(error)
                .with_bearer_challenge()
        })
}
