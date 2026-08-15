mod login;

use axum::{Router, routing::post};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login::login_handler))
}
