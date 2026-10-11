use crate::state::AppState;
use axum::{Router, routing::get};

async fn healthz() -> &'static str {
    "OK"
}

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}
