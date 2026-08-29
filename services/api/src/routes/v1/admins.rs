use axum::{Router, middleware, routing::get};

use crate::{handlers::admin::AdminHandler, middleware::auth::auth_middleware, state::AppState};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(AdminHandler::list_admins))
        .route_layer(middleware::from_fn_with_state(state, auth_middleware))
}
