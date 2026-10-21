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
    let browser = state.settings.browser.clone();
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
        .nest("/v1", v1::router(&state))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            browser,
            crate::middleware::cookies::browser_guard,
        ))
        .layer(cors)
}
