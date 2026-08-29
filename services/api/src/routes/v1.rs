mod admins;
mod auth;

use crate::{middleware::auth::auth_middleware, state::AppState};
use axum::{Router, middleware};

pub fn router(state: AppState) -> Router<AppState> {
    let public_routes = Router::new().nest("/auth", auth::public_router());

    let protected_routes = Router::new()
        .nest("/auth", auth::authed_router())
        .nest("/admins", admins::router())
        .layer(middleware::from_fn_with_state(state, auth_middleware));

    public_routes.merge(protected_routes)
}
