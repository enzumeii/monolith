mod paypal;
use crate::{handlers::paypal_payments, state::AppState};
use axum::{Router, routing::post};
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/paypal", post(paypal_payments::create))
        .nest("/paypal", paypal::router())
}
