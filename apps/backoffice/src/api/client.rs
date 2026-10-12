#![cfg(feature = "server")]

use dioxus::prelude::ServerFnError;

async fn api_error(resp: reqwest::Response) -> ServerFnError {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    ServerFnError::ServerError {
        message: format!("upstream request failed ({status}): {text}"),
        code: status.as_u16(),
        details: None,
    }
}

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
