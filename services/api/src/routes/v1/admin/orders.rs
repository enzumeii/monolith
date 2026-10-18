use crate::{handlers::orders, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new().route("/report", get(orders::report))
}
