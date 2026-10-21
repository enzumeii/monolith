use crate::{handlers::cron::configs, state::AppState};
use axum::{
    Router,
    routing::{delete, get, patch},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(configs::list).post(configs::create))
        .route("/preview", get(configs::preview))
        .route("/{id}", get(configs::get))
        .route("/{id}", patch(configs::update))
        .route("/{id}", delete(configs::delete))
}
