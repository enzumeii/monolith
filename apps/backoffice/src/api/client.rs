//! Server-only HTTP + session helpers shared by the API submodules.
//!
//! This module is only compiled for the `server` feature (the upstream base
//! URL, `reqwest`, and refresh logic are server-side concerns).

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::ServerFnError;
use domain::dto::{RefreshRequest, TokenResponse};

use crate::config::settings;
use crate::session::Session;

/// Build an absolute upstream URL from a `/v1`-relative path.
pub(crate) fn url(path: &str) -> String {
    format!("{}{}", settings().upstream.api_base, path)
}

pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().cast_signed())
}

/// Map a failed upstream response to a `ServerFnError` carrying the status
/// code so the client can distinguish session failures (401) from others.
async fn api_error(resp: reqwest::Response) -> ServerFnError {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    ServerFnError::ServerError {
        message: format!("upstream request failed ({status}): {text}"),
        code: status.as_u16(),
        details: None,
    }
}

/// Decode a successful JSON response, or map a failure to a status-coded error.
pub(crate) async fn decode<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
    what: &str,
) -> Result<T, ServerFnError> {
    if !resp.status().is_success() {
        return Err(api_error(resp).await);
    }
    resp.json::<T>()
        .await
        .map_err(|e| ServerFnError::new(format!("failed to decode {what}: {e}")))
}

/// Exchange the refresh token for a fresh pair. The refresh token is
/// single-use and rotates: the new response's refresh token replaces the old.
/// A rejected refresh is unrecoverable — the client must sign in again.
pub(crate) async fn do_refresh(session: Session) -> Result<Session, ServerFnError> {
    let resp = reqwest::Client::new()
        .post(url("/auth/refresh"))
        .json(&RefreshRequest {
            refresh_token: session.refresh_token.clone(),
        })
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(ServerFnError::ServerError {
            message: "session expired; please log in again".to_string(),
            code: 401,
            details: None,
        });
    }
    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("failed to decode refresh response: {e}")))?;

    Ok(Session {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: now_unix() + tokens.expires_in,
        admin_id: session.admin_id,
        username: session.username,
        role: session.role,
    })
}

/// Return a session whose access token is still valid, refreshing first if it
/// has expired.
pub(crate) async fn ensure_valid(session: Session) -> Result<Session, ServerFnError> {
    if now_unix() >= session.expires_at {
        do_refresh(session).await
    } else {
        Ok(session)
    }
}
