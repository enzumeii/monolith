mod inventories;
mod orders;

use crate::{middleware::token::auth_middleware, state::AppState};
use axum::{Router, middleware::from_fn_with_state};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .layer(from_fn_with_state(state, auth_middleware))
}
