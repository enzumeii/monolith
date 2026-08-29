use axum::{
    Router,
    routing::{delete, get, patch, post},
};

use crate::{handlers::admin::AdminHandler, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(AdminHandler::list))
        .route("/{id}", get(AdminHandler::get))
        .route("/{id}", patch(AdminHandler::update))
}

pub fn super_admin_router() -> Router<AppState> {
    Router::new()
        .route("/", post(AdminHandler::add))
        .route("/{id}", delete(AdminHandler::delete))
}
