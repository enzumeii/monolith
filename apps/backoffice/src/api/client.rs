#![cfg(feature = "server")]
//! Server-only HTTP helpers shared by the API submodules.
//!
//! Generic, session-less plumbing only: building an upstream URL, reading the
//! clock, and decoding a JSON response into a status-coded error. Auth/session
//! logic (refresh, `ensure_valid`) lives in [`super::auth`].

use std::time::{SystemTime, UNIX_EPOCH};

use dioxus::prelude::ServerFnError;

use crate::config::settings;

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
