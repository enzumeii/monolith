pub mod conventions;

use domain::dto::ErrorResponse;
use reqwest::{Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone, Debug, PartialEq)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
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
    let response = request.send().await.map_err(|_| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "The shop is temporarily unavailable. Please retry.",
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        let message = response.json::<ErrorResponse>().await.map_or_else(
            |_| "Unable to complete the request. Please retry.".into(),
            |body| body.error,
        );
        return Err(ApiError::new(status, message));
    }
    response.json().await.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "The shop returned an invalid response. Please retry.",
        )
    })
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status: status.as_u16(),
            message: message.into(),
        }
    }
}
