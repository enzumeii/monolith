use crate::{handlers::checkouts, state::AppState};
use axum::{Router, routing::post};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/{checkout_id}/verify-payment",
        post(checkouts::verify_payment),
    )
}
