use crate::{handlers, state::AppState};
use axum::{
    Router,
    routing::{delete, get, patch},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::cron::configs::list).post(handlers::cron::configs::create),
        )
        .route("/preview", get(handlers::cron::configs::preview))
        .route("/{id}", get(handlers::cron::configs::get))
        .route("/{id}", patch(handlers::cron::configs::update))
        .route("/{id}", delete(handlers::cron::configs::delete))
}
