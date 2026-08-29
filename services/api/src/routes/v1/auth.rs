use crate::{handlers::auth::AuthHandler, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(AuthHandler::login))
        .route("/refresh", post(AuthHandler::refresh))
}

pub fn authed_router() -> Router<AppState> {
    Router::new().route("/logout", get(AuthHandler::logout))
}
