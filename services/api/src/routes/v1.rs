mod admins;
mod auth;

use crate::{middleware::auth::Middleware, state::AppState};
use axum::{Router, middleware};

pub fn router(state: AppState) -> Router<AppState> {
    let public_routes = Router::new().nest("/auth", auth::public_router());

    let super_admin_routes = Router::new()
        .nest("/admins", admins::super_admin_router())
        .layer(middleware::from_fn(Middleware::require_super_admin));

    let protected_routes = Router::new()
        .nest("/auth", auth::authed_router())
        .nest("/admins", admins::router())
        .merge(super_admin_routes)
        .layer(middleware::from_fn_with_state(
            state,
            Middleware::auth_middleware,
        ));

    public_routes.merge(protected_routes)
}
