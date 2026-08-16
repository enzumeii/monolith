use axum::{Router, routing::get};

use crate::state::AppState;

async fn healthz() -> &'static str {
    "OK"
}

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}
