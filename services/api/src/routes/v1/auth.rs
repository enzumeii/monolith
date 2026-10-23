use crate::{handlers::auth, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(auth::login))
        .route("/refresh", post(auth::refresh))
}

pub fn authed_router() -> Router<AppState> {
    Router::new()
        .route("/session", get(auth::session))
        .route("/logout", post(auth::logout))
}
