pub mod admins;
pub mod auth;
pub mod inventories;
pub mod reports;
pub use admins::{create_admin, delete_admin, list_admins, update_admin};
pub use auth::{login, logout, refresh_session};

use domain::dto::ErrorResponse;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;

pub type ApiResult<T> = Result<T, ApiError>;
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    message: String,
}
impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status: status.as_u16(),
            message: message.into(),
        }
    }
}

pub(crate) fn valid_location_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()) && !id.bytes().all(|c| c == b'0')
}

impl std::error::Error for ApiError {}
fn request(method: Method, path: &str) -> ApiResult<RequestBuilder> {
    let settings =
        dioxus::prelude::try_consume_context::<crate::config::Settings>().ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Configuration is not loaded",
            )
        })?;
    let base = settings.api.base_url.trim_end_matches('/');
    let request = reqwest::Client::new()
        .request(method, format!("{base}{path}"))
        .header("x-session-mode", "cookie");
    #[cfg(target_arch = "wasm32")]
    let request = request.fetch_credentials_include();
    Ok(request)
}
async fn send(request: RequestBuilder) -> ApiResult<Response> {
    let response = request
        .send()
        .await
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "API unavailable"))?;
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let message = response
        .json::<ErrorResponse>()
        .await
        .map_or_else(|_| "Request failed".into(), |body| body.error);
    Err(ApiError { status, message })
}

async fn decode<T: DeserializeOwned>(request: RequestBuilder) -> ApiResult<T> {
    send(request)
        .await?
        .json()
        .await
        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "Invalid API response"))
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(test)]
#[path = "tests/api.rs"]
mod tests;
