use crate::{handlers, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{inventory_id}", get(handlers::inventories::get))
        .route(
            "/{inventory_id}/enable",
            post(handlers::inventories::post_enable),
        )
}
