use crate::{handlers::checkout, state::AppState};
use axum::{Router, extract::DefaultBodyLimit, routing::post};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/checkout", post(checkout::create))
        .route("/{checkout_id}/verify", post(checkout::verify))
        .route("/webhook/{provider}", post(checkout::webhook))
        .layer(DefaultBodyLimit::max(1_048_576))
}
