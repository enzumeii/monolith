mod admin;

use axum::{Router, routing::get};

use crate::state::AppState;

async fn healthz() -> &'static str {
    "OK"
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/admin", admin::router())
}
