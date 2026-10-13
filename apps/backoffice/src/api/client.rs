#![cfg(feature = "server")]
use dioxus::prelude::*;

pub(crate) fn unavailable() -> ServerFnError {
    failure(503, "Authentication service temporarily unavailable")
}
fn failure(code: u16, message: &str) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}
pub(crate) fn status_error(code: u16) -> ServerFnError {
    failure(
        code,
        match code {
            400 => "Invalid request",
            401 => "Invalid credentials or expired session",
            403 => "Not authorized",
            404 => "Record not found",
            409 => "Username already exists",
            _ => "Service temporarily unavailable",
        },
    )
}
pub(crate) async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    _what: &str,
) -> ServerFnResult<T> {
    if !response.status().is_success() {
        return Err(status_error(response.status().as_u16()));
    }
    response
        .json()
        .await
        .map_err(|_| failure(502, "Invalid response from upstream service"))
}
