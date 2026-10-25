use crate::{handlers, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::auth::login))
        .route("/refresh", post(handlers::auth::refresh))
}

pub fn authed_router() -> Router<AppState> {
    Router::new()
        .route("/session", get(handlers::auth::session))
        .route("/logout", post(handlers::auth::logout))
}
