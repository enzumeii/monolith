use crate::{
    error::AppError,
    middleware::{cookies, token},
    models::{AuthSource, AuthenticatedAdmin},
    services,
    state::AppState,
};
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use domain::dto::{LoginRequest, RefreshRequest, SessionResponse, TokenResponse};

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, AppError> {
    if let Some(token) = token::bearer(&headers)? {
        token::validate_token(token, &state.jwt_key)?;
    }
    let tokens = services::auth::login(&state.pool, state.jwt_key.as_slice(), payload).await?;
    if cookies::cookie_mode(&headers)? {
        cookie_response(&state, jar, tokens).await
    } else {
        Ok(Json(tokens).into_response())
    }
}
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    payload: Option<Json<RefreshRequest>>,
) -> Result<Response, AppError> {
    if let Some(token) = token::bearer(&headers)? {
        token::validate_token(token, &state.jwt_key)?;
    }
    let browser = cookies::cookie_mode(&headers)?;
    let request = if browser {
        RefreshRequest {
            refresh_token: cookies::cookie_credentials(&jar, &state.settings.browser)?
                .1
                .to_string(),
        }
    } else {
        payload
            .ok_or_else(|| AppError::BadRequest("Refresh token is required".into()))?
            .0
    };
    let tokens = services::auth::refresh(&state.pool, state.jwt_key.as_slice(), request).await?;
    if browser {
        cookie_response(&state, jar, tokens).await
    } else {
        Ok(Json(tokens).into_response())
    }
}
pub async fn logout(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedAdmin>,
    Extension(source): Extension<AuthSource>,
    jar: CookieJar,
    payload: Option<Json<RefreshRequest>>,
) -> Result<Response, AppError> {
    let request = if source == AuthSource::Cookie {
        RefreshRequest {
            refresh_token: cookies::cookie_credentials(&jar, &state.settings.browser)?
                .1
                .to_string(),
        }
    } else {
        payload
            .ok_or_else(|| AppError::BadRequest("Refresh token is required".into()))?
            .0
    };
    services::auth::logout(&state.pool, &actor, request).await?;
    if source == AuthSource::Cookie {
        Ok((
            cookies::clear_cookie(jar, &state.settings.browser),
            StatusCode::NO_CONTENT,
        )
            .into_response())
    } else {
        Ok(StatusCode::NO_CONTENT.into_response())
    }
}
async fn cookie_response(
    state: &AppState,
    jar: CookieJar,
    tokens: TokenResponse,
) -> Result<Response, AppError> {
    let claims = token::validate_token(&tokens.access_token, &state.jwt_key)?;
    let admin = services::admin::get(&state.pool, tokens.id).await?;
    let session = SessionResponse {
        admin_id: admin.id,
        username: admin.username,
        role: admin.role,
        expires_at: claims.expiration,
    };
    Ok((
        cookies::save_cookie(jar, &state.settings.browser, &tokens),
        Json(session),
    )
        .into_response())
}
