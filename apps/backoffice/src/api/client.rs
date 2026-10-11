#![cfg(feature = "server")]
//! Server-only HTTP + session helpers shared by the API submodules.
//!
//! Generic plumbing: building an upstream URL, reading the clock, decoding a
//! JSON response, and refreshing the session when its access token expires.
//! The actual `POST /v1/auth/refresh` endpoint call lives in [`super::auth`];
//! [`ensure_valid`] here just delegates to it.

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::ServerFnError;

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

/// Return a session whose access token is still valid, refreshing first (via
/// [`super::auth::refresh`]) if it has expired.
pub(crate) async fn ensure_valid(session: Session) -> Result<Session, ServerFnError> {
    if now_unix() >= session.expires_at {
        super::auth::refresh(session).await
    } else {
        Ok(session)
    }
}
