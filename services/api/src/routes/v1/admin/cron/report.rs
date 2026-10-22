use crate::{handlers::cron::report, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(report::list))
        .route("/{id}", get(report::detail))
}
