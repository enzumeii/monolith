use axum::{Router, routing::post};

use crate::{handlers::auth::AuthHandler, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(AuthHandler::login))
        .route("/refresh", post(AuthHandler::refresh))
}
