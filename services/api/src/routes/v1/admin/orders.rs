use crate::{handlers, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/report", get(handlers::orders::report))
        .route("/", get(handlers::orders::list))
        .route("/shipping_label", post(handlers::orders::shipping_labels))
        .route("/{order_id}/fulfill", post(handlers::orders::fulfill))
        .route("/{order_number}/reship", post(handlers::orders::reship))
}
