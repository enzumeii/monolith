use crate::{handlers::cron_configurations, state::AppState};
use axum::{
    Router,
    routing::{delete, get, patch},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(cron_configurations::list).post(cron_configurations::create),
        )
        .route("/preview", get(cron_configurations::preview))
        .route("/{id}", get(cron_configurations::get))
        .route("/{id}", patch(cron_configurations::update))
        .route("/{id}", delete(cron_configurations::delete))
}
