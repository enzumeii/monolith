use crate::{handlers::inventories, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{inventory_id}", get(inventories::get))
        .route("/{inventory_id}/enable", post(inventories::post_enable))
}
