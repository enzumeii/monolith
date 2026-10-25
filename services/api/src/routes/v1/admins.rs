use crate::{handlers, state::AppState};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::admin::list))
        .route("/{id}", get(handlers::admin::get))
        .route("/{id}", patch(handlers::admin::update))
}

pub fn super_admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(handlers::admin::add))
        .route("/{id}", delete(handlers::admin::delete))
}
