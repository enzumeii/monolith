mod cron;
mod healthz;
mod v1;

use crate::state::AppState;
use axum::Router;

/// Build the API router.
///
/// # Panics
/// Panics if the browser origin was not validated during startup.
pub fn router(state: AppState) -> Router {
    let browser = state.settings.browser.clone();
    let origins = browser
        .origins()
        .map(|origin| {
            origin
                .parse::<axum::http::HeaderValue>()
                .expect("validated browser origin")
        })
        .collect::<Vec<_>>();
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
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
        ])
        .expose_headers([axum::http::header::CONTENT_DISPOSITION]);
    let cron =
        if state.settings.scheduler.implementation == crate::config::CronImplementation::Vercel {
            cron::router()
        } else {
            Router::new()
        };
    Router::new()
        .merge(healthz::router())
        .merge(cron)
        .nest("/api/v1", v1::router(&state))
        .nest("/v1", v1::router(&state))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            browser,
            crate::middleware::cookies::browser_guard,
        ))
        .layer(cors)
}
