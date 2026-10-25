use crate::{handlers, state::AppState};
use axum::{Router, extract::DefaultBodyLimit, routing::post};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/checkout", post(handlers::checkout::create))
        .route("/{checkout_id}/verify", post(handlers::checkout::verify))
        .route("/webhook/{provider}", post(handlers::checkout::webhook))
        .layer(DefaultBodyLimit::max(1_048_576))
}
