use crate::{handlers, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::cron::report::list))
        .route("/{id}", get(handlers::cron::report::detail))
        .route(
            "/{id}/files/{filename}/download-url",
            get(handlers::cron::report::download_url),
        )
}
