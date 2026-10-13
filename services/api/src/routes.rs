mod healthz;
mod v1;

use crate::state::AppState;
use axum::Router;

/// Build the API router.
///
/// # Panics
/// Panics if the browser origin was not validated during startup.
pub fn router(state: AppState) -> Router {
    let origin = state
        .settings
        .browser
        .public_origin
        .parse::<axum::http::HeaderValue>()
        .expect("validated browser origin");
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(origin)
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static(crate::middleware::cookies::MODE_HEADER),
        ]);
    Router::new()
        .merge(healthz::router())
        .nest("/v1", v1::router(state.clone()))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::middleware::cookies::browser_guard,
        ))
        .layer(cors)
}
