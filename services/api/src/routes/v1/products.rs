use crate::{handlers::products, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(products::get))
}
