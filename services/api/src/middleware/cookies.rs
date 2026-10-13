use crate::{config::BrowserConfig, error::AppError, state::AppState};
use axum::{
    extract::{Request, State},
    http::{
        HeaderMap, Method,
        header::{AUTHORIZATION, CACHE_CONTROL, ORIGIN},
    },
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use domain::dto::TokenResponse;

pub(crate) const MODE_HEADER: &str = "x-session-mode";

pub(crate) fn cookie_mode(headers: &HeaderMap) -> Result<bool, AppError> {
    match headers.get(MODE_HEADER) {
        None => Ok(false),
        Some(value) if value == "cookie" => Ok(true),
        Some(_) => Err(AppError::BadRequest("Invalid session mode".into())),
    }
}
pub(crate) fn cookie_credentials<'a>(
    jar: &'a CookieJar,
    config: &BrowserConfig,
) -> Result<(&'a str, &'a str), AppError> {
    let value = jar
        .get(cookie_name(config))
        .ok_or(AppError::InvalidToken)?
        .value();
    let (access, refresh) = value.split_once('~').ok_or(AppError::InvalidToken)?;
    if access.is_empty()
        || access.len() > 3500
        || refresh.len() != 64
        || !refresh.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AppError::InvalidToken);
    }
    Ok((access, refresh))
}
pub(crate) fn save_cookie(
    jar: CookieJar,
    config: &BrowserConfig,
    tokens: &TokenResponse,
) -> CookieJar {
    jar.add(
        Cookie::build((
            cookie_name(config),
            format!("{}~{}", tokens.access_token, tokens.refresh_token),
        ))
        .http_only(true)
        .secure(!config.insecure_local)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(
            std::time::Duration::from_hours(168)
                .try_into()
                .expect("seven days fits the cookie duration"),
        )
        .build(),
    )
}
pub(crate) fn clear_cookie(jar: CookieJar, config: &BrowserConfig) -> CookieJar {
    jar.remove(
        Cookie::build(cookie_name(config))
            .path("/")
            .http_only(true)
            .secure(!config.insecure_local)
            .same_site(SameSite::Lax)
            .build(),
    )
}
pub(crate) async fn browser_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let browser = cookie_mode(request.headers())?
        || (!request.headers().contains_key(AUTHORIZATION)
            && CookieJar::from_headers(request.headers())
                .get(cookie_name(&state.settings.browser))
                .is_some());
    if browser
        && !matches!(
            *request.method(),
            Method::GET | Method::HEAD | Method::OPTIONS
        )
        && request
            .headers()
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok())
            != Some(state.settings.browser.public_origin.as_str())
    {
        return Err(AppError::Forbidden);
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, "no-store".parse().expect("static header"));
    Ok(response)
}

fn cookie_name(config: &BrowserConfig) -> &'static str {
    if config.insecure_local {
        "backoffice_auth"
    } else {
        "__Host-backoffice_auth"
    }
}

#[cfg(test)]
#[path = "../tests/middleware/cookies.rs"]
mod tests;
