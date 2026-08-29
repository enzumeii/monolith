use axum::{Router, routing::get};

use crate::{handlers::admin::AdminHandler, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(AdminHandler::list_admins))
}
