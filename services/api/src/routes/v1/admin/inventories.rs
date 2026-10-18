use crate::{handlers::inventories, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/{inventory_id}", get(inventories::get))
}
