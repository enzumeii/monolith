pub mod conventions;
pub mod orders;

use domain::dto::ErrorResponse;
use reqwest::{Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone, Debug, PartialEq)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    transient: bool,
}

pub fn request(method: Method, path: &str) -> ApiResult<RequestBuilder> {
    let settings =
        dioxus::prelude::try_consume_context::<crate::config::Settings>().ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Configuration is not loaded",
            )
        })?;
    let request =
        reqwest::Client::new().request(method, format!("{}{path}", settings.api.base_url));
    #[cfg(target_arch = "wasm32")]
    let request = request.fetch_credentials_omit();
    Ok(request)
}

pub async fn decode<T: DeserializeOwned>(request: RequestBuilder) -> ApiResult<T> {
    let response = request
        .send()
        .await
        .map_err(|error| ApiError::transport(&error, "The shop is temporarily unavailable."))?;
    let status = response.status();
    if !status.is_success() {
        let message = response.json::<ErrorResponse>().await.map_or_else(
            |_| "Unable to complete the request.".into(),
            |body| body.error,
        );
        return Err(ApiError::http(status, message));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| ApiError::transport(&error, "The shop response was interrupted."))?;
    serde_json::from_slice(&bytes).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "The shop returned an invalid response.",
        )
    })
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status: status.as_u16(),
            message: message.into(),
            transient: false,
        }
    }

    pub fn http(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            transient: matches!(status.as_u16(), 408 | 429 | 500..=599),
            ..Self::new(status, message)
        }
    }

    pub fn is_transient(&self) -> bool {
        self.transient
    }

    pub(crate) fn transport(error: &reqwest::Error, message: impl Into<String>) -> Self {
        Self {
            transient: !error.is_builder() && !error.is_redirect(),
            ..Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
        }
    }
}

pub async fn retry<T, F, Fut>(mut operation: F) -> ApiResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ApiResult<T>>,
{
    let mut failures: u32 = 0;
    loop {
        match operation().await {
            Err(error) if error.is_transient() => {
                failures = failures.saturating_add(1);
                if !delay(backoff_seconds(failures)).await {
                    return Err(ApiError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Automatic recovery is unavailable in this browser.",
                    ));
                }
            }
            result => return result,
        }
    }
}

pub fn backoff_seconds(failure_count: u32) -> u32 {
    match failure_count {
        0 | 1 => 5,
        2 => 10,
        3 => 20,
        _ => 30,
    }
}

pub async fn delay(seconds: u32) -> bool {
    let millis = seconds.saturating_mul(1000);
    dioxus::document::eval(&format!(
        "await new Promise(resolve => setTimeout(resolve, {millis})); dioxus.send(true);"
    ))
    .recv::<bool>()
    .await
    .unwrap_or(false)
}

#[cfg(test)]
#[path = "tests/api.rs"]
pub(crate) mod tests;
