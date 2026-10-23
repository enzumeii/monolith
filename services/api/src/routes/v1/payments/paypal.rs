use crate::{handlers::paypal_payments, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(paypal_payments::get))
        .route("/{id}/capture", post(paypal_payments::capture))
}
