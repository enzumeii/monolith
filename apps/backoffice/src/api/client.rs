#![cfg(feature = "server")]
//! Server-only HTTP plumbing shared by the API endpoint wrappers.

use dioxus::prelude::ServerFnError;

use crate::config::settings;

pub(crate) fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build upstream HTTP client")
    })
}

pub(crate) fn url(path: &str) -> String {
    format!("{}{}", settings().upstream.api_base, path)
}

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
