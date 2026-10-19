use crate::{handlers::orders, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/report", get(orders::report))
        .route("/", get(orders::list))
        .route("/shipping_label", post(orders::shipping_labels))
        .route("/{order_id}/fulfill", post(orders::fulfill))
        .route("/{order_number}/reship", post(orders::reship))
}
