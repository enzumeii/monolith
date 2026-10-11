use crate::{handlers::admin, state::AppState};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(admin::list))
        .route("/{id}", get(admin::get))
        .route("/{id}", patch(admin::update))
}

pub fn super_admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(admin::add))
        .route("/{id}", delete(admin::delete))
}
