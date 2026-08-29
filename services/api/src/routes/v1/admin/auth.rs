use axum::{Router, routing::post};

use crate::{handlers::admin::AdminHandler, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/login", post(AdminHandler::login_handler))
}
