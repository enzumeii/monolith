use crate::{error::AppError, state::AppState};
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::get,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/_cron",
            get(invoke).head(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .layer(axum::middleware::map_response(
            |mut response: axum::response::Response| async move {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("no-store"),
                );
                response
            },
        ))
}

async fn invoke(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, AppError> {
    let secret = state
        .settings
        .cron_secret
        .as_deref()
        .filter(|secret| !secret.trim().is_empty())
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Cron authentication unavailable",
            )
        })?;
    let expected = format!("Bearer {secret}");
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if authorization != Some(expected.as_str()) {
        return Err(
            AppError::new(StatusCode::UNAUTHORIZED, "Not authorized").with_bearer_challenge()
        );
    }
    crate::CronRuntime::tick(&crate::VercelCron, &state).await?;
    Ok(StatusCode::NO_CONTENT)
}
