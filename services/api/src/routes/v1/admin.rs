use crate::handlers::admin::AdminHandler;
use crate::state::AppState;
use axum::{Router, routing::post};

pub fn router() -> Router<AppState> {
    Router::new().route("/login", post(AdminHandler::login_handler))
}
