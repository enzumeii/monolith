use crate::{
    client::ClientError,
    error::{AppError, client_timeout},
};
use axum::http::StatusCode;

#[track_caller]
pub(super) fn upstream(error: ClientError) -> AppError {
    let status = if client_timeout(&error) {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::BAD_GATEWAY
    };
    AppError::new(status, "Upstream request failed").with_source(error)
}

#[cfg(test)]
#[path = "../tests/services/common.rs"]
pub(super) mod tests;
